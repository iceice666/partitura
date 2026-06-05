defmodule Harmony.ConfigTest do
  @moduledoc "Task 2.5: global/project precedence, defaults, and mode validation."
  use ExUnit.Case, async: false

  import Harmony.TestHelpers

  setup do
    # Each test starts its own Config instance with a unique config_path so
    # tests don't share state.
    tmp = Path.join(System.tmp_dir!(), "harmony_cfg_#{System.unique_integer([:positive])}")
    File.mkdir_p!(tmp)
    global_config_path = Path.join(tmp, "config.yaml")

    {:ok, pid} = start_supervised({Harmony.Config, [config_path: global_config_path]})

    on_exit(fn -> File.rm_rf!(tmp) end)

    {:ok, tmp: tmp, global_config_path: global_config_path, pid: pid}
  end

  # ── Defaults when config absent ──────────────────────────────────────────────

  test "built-in defaults when global config file does not exist" do
    assert Harmony.Config.max_retries() == 2
    assert Harmony.Config.max_verify_cycles() == 3
  end

  test "verify_loop defaults to false for any project" do
    project_id = "proj-#{System.unique_integer([:positive])}"
    repo = make_git_repo()
    on_exit(fn -> File.rm_rf!(repo) end)

    Harmony.Config.register_project(project_id, repo)
    refute Harmony.Config.verify_loop?(project_id)
  end

  test "default WIP limits" do
    limits = Harmony.Config.wip_limits()
    assert limits["building"] == 4
    assert limits["reviewing"] == 6
    assert limits["human_inbox"] == 3
  end

  # ── Global config loading ────────────────────────────────────────────────────

  test "global config is read at startup", %{global_config_path: path} do
    # Write a new global config and restart Config with that path
    write_yaml_config(path, %{"max_retries" => 5, "max_verify_cycles" => 7})

    # Restart with the same path so it picks up the file
    stop_supervised(Harmony.Config)
    {:ok, _} = start_supervised({Harmony.Config, [config_path: path]})

    assert Harmony.Config.max_retries() == 5
    assert Harmony.Config.max_verify_cycles() == 7
  end

  test "api_token is read from global config", %{global_config_path: path} do
    write_yaml_config(path, %{"api_token" => "supersecret"})
    stop_supervised(Harmony.Config)
    {:ok, _} = start_supervised({Harmony.Config, [config_path: path]})

    assert Harmony.Config.api_token() == "supersecret"
  end

  test "api_token is nil when absent from global config" do
    assert Harmony.Config.api_token() == nil
  end

  # ── Project config and precedence ────────────────────────────────────────────

  test "project mode and verify_loop are read from project config" do
    repo = make_git_repo()
    on_exit(fn -> File.rm_rf!(repo) end)

    project_id = "proj-#{System.unique_integer([:positive])}"

    write_yaml_config(
      Path.join([repo, ".score", "config.yaml"]),
      %{"mode" => "warm", "verify_loop" => true}
    )

    Harmony.Config.register_project(project_id, repo)

    assert Harmony.Config.project_mode(project_id) == "warm"
    assert Harmony.Config.verify_loop?(project_id) == true
  end

  test "project max_verify_cycles overrides global", %{global_config_path: path} do
    write_yaml_config(path, %{"max_verify_cycles" => 3})
    stop_supervised(Harmony.Config)
    {:ok, _} = start_supervised({Harmony.Config, [config_path: path]})

    repo_a = make_git_repo()
    repo_b = make_git_repo()

    on_exit(fn ->
      File.rm_rf!(repo_a)
      File.rm_rf!(repo_b)
    end)

    id_a = "proj-a-#{System.unique_integer([:positive])}"
    id_b = "proj-b-#{System.unique_integer([:positive])}"

    write_yaml_config(
      Path.join([repo_a, ".score", "config.yaml"]),
      %{"max_verify_cycles" => 5}
    )

    Harmony.Config.register_project(id_a, repo_a)
    Harmony.Config.register_project(id_b, repo_b)

    # Project A overrides to 5; project B falls through to global 3
    assert Harmony.Config.max_verify_cycles(id_a) == 5
    assert Harmony.Config.max_verify_cycles(id_b) == 3
  end

  # ── Mode validation ───────────────────────────────────────────────────────────

  test "all valid modes are accepted" do
    for mode <- ~w(hot warm cold frozen maintenance) do
      assert {:ok, ^mode} = Harmony.Config.validate_mode(mode)
    end
  end

  test "invalid mode returns error" do
    assert {:error, _msg} = Harmony.Config.validate_mode("turbo")
  end

  test "register_project returns error for invalid mode" do
    repo = make_git_repo()
    on_exit(fn -> File.rm_rf!(repo) end)

    write_yaml_config(
      Path.join([repo, ".score", "config.yaml"]),
      %{"mode" => "bogus"}
    )

    project_id = "proj-#{System.unique_integer([:positive])}"
    assert {:error, _} = Harmony.Config.register_project(project_id, repo)
  end

  # ── Dispatch permission by mode ───────────────────────────────────────────────

  test "dispatch_allowed? follows project mode" do
    repo = make_git_repo()
    on_exit(fn -> File.rm_rf!(repo) end)

    for {mode, allowed} <- [
          {"hot", true},
          {"warm", true},
          {"cold", false},
          {"frozen", false}
        ] do
      write_yaml_config(Path.join([repo, ".score", "config.yaml"]), %{"mode" => mode})
      stop_supervised(Harmony.Config)
      {:ok, _} = start_supervised(Harmony.Config)

      project_id = "proj-#{System.unique_integer([:positive])}"
      Harmony.Config.register_project(project_id, repo)

      assert Harmony.Config.dispatch_allowed?(project_id) == allowed,
             "Expected dispatch_allowed? == #{allowed} for mode #{mode}"
    end
  end

  test "maintenance mode allows hot-fix tagged tickets" do
    repo = make_git_repo()
    on_exit(fn -> File.rm_rf!(repo) end)

    write_yaml_config(
      Path.join([repo, ".score", "config.yaml"]),
      %{"mode" => "maintenance"}
    )

    project_id = "proj-#{System.unique_integer([:positive])}"
    Harmony.Config.register_project(project_id, repo)

    refute Harmony.Config.dispatch_allowed?(project_id, [])
    assert Harmony.Config.dispatch_allowed?(project_id, ["hot-fix"])
  end
end
