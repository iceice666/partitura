import Foundation

enum PreviewFixtures {
    static let daemons = [
        DaemonConfig(id: "local", name: "Local Harmony", url: URL(string: "ws://localhost:4242/socket")!, token: "local-dev"),
        DaemonConfig(id: "staging", name: "Staging Harmony", url: URL(string: "ws://localhost:4343/socket")!, token: "staging")
    ]

    static let projects = [
        Project(id: "partitura", name: "partitura", mode: .hot, ticketCount: 9),
        Project(id: "aria", name: "aria", mode: .warm, ticketCount: 6),
        Project(id: "harmony", name: "harmony", mode: .warm, ticketCount: 5),
        Project(id: "voice", name: "voice", mode: .maintenance, ticketCount: 3),
        Project(id: "echo", name: "echo", mode: .cold, ticketCount: 2)
    ]

    static let providers = [
        Provider(id: "anthropic", name: "Anthropic", hasCredentials: true, models: ["claude-opus-4-8", "claude-sonnet-4-7"]),
        Provider(id: "openai", name: "OpenAI", hasCredentials: true, models: ["gpt-4o", "o3"]),
        Provider(id: "google", name: "Google", hasCredentials: false, models: ["gemini-2.5-pro"])
    ]

    static let roles = [
        Role(id: "builder", name: "builder", providerID: "anthropic", model: "claude-opus-4-8"),
        Role(id: "planner", name: "planner", providerID: "anthropic", model: "claude-sonnet-4-7"),
        Role(id: "reviewer", name: "reviewer", providerID: "openai", model: "o3"),
        Role(id: "researcher", name: "researcher", providerID: "google", model: "gemini-2.5-pro")
    ]

    static let tickets: [Ticket] = [
        ticket("draft-harmony-reconnect", "Draft reconnect banner copy", .pitched, tags: ["ux"], assignee: "@me"),
        ticket("implement-aria-macos-ui", "Implement native macOS UI", .building, tags: ["macos", "swiftui"], assignee: "@builder", active: true),
        ticket("verify-live-log", "Verify live log tailing behavior", .reviewing, tags: ["voice", "logs"], assignee: "@reviewer"),
        ticket("provider-credential-sheet", "Add provider credential state sheet", .ready, tags: ["runtime"], assignee: "@builder"),
        ticket("needs-token-input", "Ask for missing API token securely", .awaitingInput, tags: ["security"], assignee: "@planner", needsInput: true),
        ticket("respec-daemon-auth", "Respec daemon auth handshake", .specced, tags: ["auth"], assignee: "@planner", infeasible: true),
        ticket("archive-old-gtk-spike", "Archive old GTK spike notes", .done, tags: ["cleanup"], assignee: "@me"),
        ticket("blocked-on-contract", "Clarify project changed payload", .blocked, tags: ["contract"], assignee: "@planner"),
        ticket("old-board-mock", "Retire old board mock", .archived, tags: ["archive"], assignee: "@me")
    ]

    static var state: AppState {
        var state = AppState()
        state.connection = .connected
        state.projects = projects
        state.activeProjectID = projects.first?.id
        state.providers = providers
        state.roles = roles
        state.daemonConfigs = daemons
        state.activeDaemonID = daemons.first?.id
        state.tickets = Dictionary(uniqueKeysWithValues: tickets.map { ($0.id, $0) })
        state.wipWarnings[.building] = 4
        return state
    }

    private static func ticket(
        _ id: TicketID,
        _ title: String,
        _ status: TicketStatus,
        tags: [String],
        assignee: String?,
        active: Bool = false,
        needsInput: Bool = false,
        infeasible: Bool = false
    ) -> Ticket {
        let run = Run(
            id: "20260607-120000-\(id.prefix(4))",
            ticketID: id,
            role: assignee?.replacingOccurrences(of: "@", with: "") ?? "builder",
            model: "anthropic/claude-opus-4-8",
            startedAt: Date().addingTimeInterval(-1800),
            finishedAt: active ? nil : Date().addingTimeInterval(-300),
            progress: [
                VoiceEvent(type: "status", text: "reading ticket context"),
                VoiceEvent(type: "tool_call", text: "rg --files"),
                VoiceEvent(type: "text", text: "applying focused implementation changes")
            ],
            report: active ? nil : RunReport(
                exitReason: infeasible ? .infeasible : .completed,
                summary: infeasible ? "Returned as infeasible pending protocol detail." : "Completed implementation and verification notes.",
                filesChanged: ["aria/macos/Sources/Aria/App.swift", "aria/macos/Sources/Aria/Store/AppStore.swift"],
                acceptance: ["SwiftPM package builds", "UI renders seven columns"],
                notes: "Fixture report used for preview and smoke verification.",
                infeasibility: infeasible ? Infeasibility(reason: "Protocol shape is incomplete.", prerequisites: ["Confirm auth token refresh"], suggestedChanges: ["Add daemon auth delta spec"]) : nil
            )
        )
        let questions = needsInput ? [
            InputQuestion(id: "api-token", label: "API token", kind: .secure, required: true, options: []),
            InputQuestion(id: "environment", label: "Environment", kind: .picker, required: true, options: ["local", "staging"])
        ] : []
        return Ticket(
            id: id,
            title: title,
            status: status,
            created: Date().addingTimeInterval(Double.random(in: -86000 ... -900)),
            assignee: assignee,
            tags: tags,
            spec: TicketSpec(
                what: "Build the requested Partitura behavior while keeping Aria a thin client over Harmony state.",
                acceptance: ["Render the requested surface", "Forward user intent through Harmony commands", "Keep controls disabled while disconnected"],
                reworkNotes: nil,
                respecNotes: infeasible ? "Needs a clearer daemon authentication contract." : nil,
                clarifications: [:],
                handoffNotes: nil
            ),
            pitch: Pitch(appetite: "medium", text: "A focused native macOS pass with stable dimensions and accessible controls."),
            blockedBy: status == .blocked ? ["provider-credential-sheet"] : [],
            runs: [run],
            activeRunID: active ? run.id : nil,
            needsInput: questions
        )
    }
}
