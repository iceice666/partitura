use std::process;
use std::time::Instant;

use echo::AbortHandle;
use tracing::error;
use voice_core::{
    ExitCode,
    context::{assemble_system_prompt, build_user_message, extract_acceptance_commands},
    env::Env,
    events,
    loop_::{LoopConfig, LoopOutcome, run_loop},
    manifest::RoleManifest,
    mcp::lifecycle::McpPool,
    model::EchoStream,
    report::{RunReport, TokenUsage},
    workspace::Workspace,
};

#[tokio::main]
async fn main() {
    // Initialise tracing to stderr only — stdout is the protocol channel.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let started_at = chrono::Utc::now();
    let start_instant = Instant::now();

    // ── 1. Validate env vars (exit 2 on failure, no setup yet) ──────────────
    let env = match Env::from_environment() {
        Ok(e) => e,
        Err(err) => {
            error!("VOICE_* env validation failed: {err}");
            process::exit(ExitCode::HardAbort.as_i32());
        }
    };

    // ── 2. Load role manifest (exit 2 on failure) ────────────────────────────
    let manifest = match RoleManifest::load(&env.role_manifest) {
        Ok(m) => m,
        Err(err) => {
            error!("role manifest invalid: {err}");
            process::exit(ExitCode::HardAbort.as_i32());
        }
    };

    // ── 3. Resolve echo model ────────────────────────────────────────────────
    let provider: echo::Provider = match manifest.model.provider.parse() {
        Ok(p) => p,
        Err(e) => {
            error!("unknown model provider '{}': {e}", manifest.model.provider);
            process::exit(ExitCode::HardAbort.as_i32());
        }
    };
    let model = match echo::get_model(provider, &manifest.model.id) {
        Some(m) => m,
        None => {
            error!("model '{}' not found in registry", manifest.model.id);
            process::exit(ExitCode::HardAbort.as_i32());
        }
    };

    // ── 4. Set up workspace (exit 2 on failure) ──────────────────────────────
    // Derive the repo root as the parent of the workspace's parent directory.
    // Convention: VOICE_WORKSPACE = <repo>/.score/workspaces/<ticket-id>
    let repo_root = env
        .workspace
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .unwrap_or(&env.workspace);
    let ticket_id = env
        .workspace
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&env.run_id);

    let workspace =
        match Workspace::setup(repo_root, &env.workspace, ticket_id, manifest.dispatch_mode) {
            Ok(w) => w,
            Err(err) => {
                error!("worktree setup failed: {err}");
                process::exit(ExitCode::HardAbort.as_i32());
            }
        };

    // ── 5. SIGTERM handler ───────────────────────────────────────────────────
    let abort = AbortHandle::new();
    let abort_for_signal = abort.clone();
    tokio::spawn(async move {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut sigterm) = signal(SignalKind::terminate()) {
            sigterm.recv().await;
            abort_for_signal.abort();
        }
    });

    // ── 6. Assemble context ──────────────────────────────────────────────────
    let system_prompt = assemble_system_prompt(&manifest, workspace.path());
    let first_user_message = match build_user_message(&env.ticket_path) {
        Ok(msg) => msg,
        Err(err) => {
            error!("cannot read ticket: {err}");
            write_hard_abort_report(
                &env,
                &manifest,
                &model,
                started_at.to_rfc3339(),
                start_instant.elapsed().as_secs_f64(),
                workspace.files_changed(),
            );
            workspace.remove(repo_root);
            process::exit(ExitCode::HardAbort.as_i32());
        }
    };
    let acceptance_commands = extract_acceptance_commands(&env.ticket_path);

    // ── 7. Launch MCP servers ────────────────────────────────────────────────
    let mcp_pool = match McpPool::start(
        &manifest.tools.mcp_servers,
        &manifest.tools.allow,
        workspace.path(),
    )
    .await
    {
        Ok(pool) => pool,
        Err(err) => {
            error!("MCP server startup failed: {err}");
            write_hard_abort_report(
                &env,
                &manifest,
                &model,
                started_at.to_rfc3339(),
                start_instant.elapsed().as_secs_f64(),
                workspace.files_changed(),
            );
            workspace.remove(repo_root);
            process::exit(ExitCode::HardAbort.as_i32());
        }
    };

    // ── 8. Run the agent loop ────────────────────────────────────────────────
    let loop_cfg = LoopConfig {
        model: model.clone(),
        system_prompt,
        first_user_message,
        acceptance_commands,
        budgets: manifest.budgets.clone(),
        abort: abort.clone(),
    };

    let outcome = run_loop(loop_cfg, &EchoStream, &mcp_pool, &manifest, &workspace).await;

    // ── 9. Tear down MCP ─────────────────────────────────────────────────────
    mcp_pool.shutdown().await;

    // ── 10. Write report and exit ────────────────────────────────────────────
    let finished_at = chrono::Utc::now();
    let duration_seconds = start_instant.elapsed().as_secs_f64();
    let files_changed = workspace.files_changed();
    let ticket_id_str = env
        .ticket_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&env.run_id)
        .to_string();

    let report_inputs = ReportInputs {
        run_id: &env.run_id,
        ticket_id: &ticket_id_str,
        manifest: &manifest,
        model_id: model.id.clone(),
        started_at: started_at.to_rfc3339(),
        finished_at: finished_at.to_rfc3339(),
        duration_seconds,
        files_changed,
    };
    let (exit_code, report) = build_report(outcome, report_inputs);

    // Worktree cleanup.
    if !exit_code.keep_worktree() {
        workspace.remove(repo_root);
    }

    // Write report (mandatory on 0/3/4; best-effort on 1/5; optional on 2).
    if exit_code != ExitCode::HardAbort
        && let Some(r) = report
        && let Err(err) = r.write_atomic(&env.report_path)
    {
        error!("failed to write run report: {err}");
    }

    process::exit(exit_code.as_i32());
}

fn write_hard_abort_report(
    env: &Env,
    manifest: &RoleManifest,
    model: &echo::Model,
    started_at: String,
    duration_seconds: f64,
    files_changed: Vec<voice_core::report::FileChange>,
) {
    let ticket_id = env
        .ticket_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&env.run_id)
        .to_string();
    let report = RunReport {
        schema: "score.run-report/v1",
        run_id: env.run_id.clone(),
        ticket_id,
        role: manifest.role.clone(),
        model: model.id.clone(),
        exit_reason: ExitCode::HardAbort.exit_reason().to_string(),
        started_at,
        finished_at: chrono::Utc::now().to_rfc3339(),
        duration_seconds,
        turns: 0,
        token_usage: TokenUsage::default(),
        files_changed,
        acceptance_results: None,
        questions: None,
        infeasibility: None,
        verdict: None,
        handoff: None,
    };
    if let Err(err) = report.write_atomic(&env.report_path) {
        error!("failed to write hard-abort report: {err}");
    }
}

struct ReportInputs<'a> {
    run_id: &'a str,
    ticket_id: &'a str,
    manifest: &'a RoleManifest,
    model_id: String,
    started_at: String,
    finished_at: String,
    duration_seconds: f64,
    files_changed: Vec<voice_core::report::FileChange>,
}

fn build_report(outcome: LoopOutcome, inputs: ReportInputs<'_>) -> (ExitCode, Option<RunReport>) {
    let base = |exit_code: ExitCode, token_usage: TokenUsage, turns: u32| RunReport {
        schema: "score.run-report/v1",
        run_id: inputs.run_id.to_string(),
        ticket_id: inputs.ticket_id.to_string(),
        role: inputs.manifest.role.clone(),
        model: inputs.model_id.clone(),
        exit_reason: exit_code.exit_reason().to_string(),
        started_at: inputs.started_at.clone(),
        finished_at: inputs.finished_at.clone(),
        duration_seconds: inputs.duration_seconds,
        turns,
        token_usage,
        files_changed: inputs.files_changed.clone(),
        acceptance_results: None,
        questions: None,
        infeasibility: None,
        verdict: None,
        handoff: None,
    };

    match outcome {
        LoopOutcome::Completed {
            acceptance,
            token_usage,
            turns,
        } => {
            let mut r = base(ExitCode::Completed, token_usage, turns);
            r.acceptance_results = Some(acceptance);
            (ExitCode::Completed, Some(r))
        }
        LoopOutcome::Infeasible {
            infeasibility,
            token_usage,
            turns,
        } => {
            let mut r = base(ExitCode::Infeasible, token_usage, turns);
            r.infeasibility = Some(infeasibility);
            (ExitCode::Infeasible, Some(r))
        }
        LoopOutcome::NeedsInput {
            questions,
            token_usage,
            turns,
        } => {
            let mut r = base(ExitCode::NeedsInput, token_usage, turns);
            r.questions = Some(questions);
            (ExitCode::NeedsInput, Some(r))
        }
        LoopOutcome::Failed {
            reason,
            handoff,
            token_usage,
            turns,
        } => {
            let mut r = base(ExitCode::Failed, token_usage, turns);
            r.handoff = handoff;
            events::emit_error(&reason);
            (ExitCode::Failed, Some(r))
        }
        LoopOutcome::Cancelled { token_usage, turns } => {
            let r = base(ExitCode::Cancelled, token_usage, turns);
            (ExitCode::Cancelled, Some(r))
        }
    }
}
