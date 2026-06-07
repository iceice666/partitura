import Foundation
import Observation

@MainActor
@Observable
final class AppStore {
    var state: AppState
    var isInspectorPresented = false
    var isNewTicketSheetPresented = false
    var isProvidersRolesSheetPresented = false
    var isDaemonSelectorPresented = false
    var isDaemonConfigPresented = false
    var isSwitchWarningPresented = false
    var daemonDraft = DaemonDraft()
    var selectedProviderID: ProviderID?
    var focusedRunID: RunID?
    var pendingDaemonSwitchID: String?

    private let harmony: HarmonyActor
    private var toastTasks: [UUID: Task<Void, Never>] = [:]

    init(state: AppState = AppState(), harmony: HarmonyActor = HarmonyActor()) {
        self.state = state
        self.harmony = harmony
        self.selectedProviderID = state.providers.first?.id
    }

    static func preview() -> AppStore {
        AppStore(state: PreviewFixtures.state)
    }

    func send(_ action: AppAction) {
        switch action {
        case .appStarted:
            state.connection = .connecting
            Task { await harmony.connect(to: state.activeDaemon, send: sendFromNetwork(_:)) }
        case .harmonyConnected:
            state.connection = .connected
            state.connectionReason = nil
            state.retryDelay = 1
            addToast(.init(kind: .daemon, title: "Connected", body: state.activeDaemon?.name ?? "Harmony", symbol: "checkmark.circle.fill"), timeout: 3)
        case let .harmonyDisconnected(reason):
            state.connection = .disconnected
            state.connectionReason = reason
            scheduleReconnect()
        case let .harmonyError(message):
            state.pendingOps.removeAll()
            addToast(.init(kind: .general, title: "Harmony Error", body: message, symbol: "exclamationmark.triangle.fill"), timeout: 5)
        case let .projectsReceived(projects):
            state.projects = projects
            if state.activeProjectID == nil {
                state.activeProjectID = projects.first?.id
            }
        case let .projectChanged(project):
            if let index = state.projects.firstIndex(where: { $0.id == project.id }) {
                state.projects[index] = project
            } else {
                state.projects.append(project)
            }
            state.pendingOps.remove(.project(project.id))
        case let .runtimesReceived(providers, roles):
            state.providers = providers
            state.roles = roles
            selectedProviderID = selectedProviderID ?? providers.first?.id
        case let .ticketChanged(ticket):
            state.tickets[ticket.id] = ticket
            state.pendingOps.remove(.ticket(ticket.id))
            state.selection = normalizedSelection(state.selection)
        case let .runStarted(run):
            upsertRun(run)
            state.pendingOps.remove(.ticket(run.ticketID))
            state.selection = .run(run.ticketID, run.id)
            isInspectorPresented = true
        case let .runProgress(runID, event):
            patchRun(runID) { $0.progress.append(event) }
        case let .runFinished(runID, report):
            patchRun(runID) { run in
                run.finishedAt = Date()
                run.report = report
            }
            if let ticketID = ticketID(for: runID) {
                let failed = report.exitReason == .failed || report.exitReason == .hardAbort || report.exitReason == .infeasible
                addToast(.init(kind: .runFinished(ticketID, runID, failed: failed), title: "Run Finished", body: report.summary, symbol: failed ? "xmark.octagon.fill" : "checkmark.seal.fill"), timeout: 5)
            }
        case let .runNeedsInput(ticketID, questions):
            if var ticket = state.tickets[ticketID] {
                ticket.status = .awaitingInput
                ticket.needsInput = questions
                state.tickets[ticketID] = ticket
                state.pendingOps.remove(.ticket(ticketID))
            }
        case let .wipWarning(status, count):
            state.wipWarnings[status] = count
        case let .inboxBlocked(blocked):
            state.inboxBlocked = blocked
        case let .selectProject(id):
            state.activeProjectID = id
            state.selection = .none
            state.tickets.removeAll()
            Task { await harmony.joinProject(id) }
        case let .selectTicket(id):
            guard state.tickets[id] != nil else {
                state.selection = .none
                isInspectorPresented = false
                return
            }
            state.selection = .ticket(id)
            isInspectorPresented = true
        case let .selectRun(ticketID, runID):
            state.selection = .run(ticketID, runID)
            focusedRunID = runID
            isInspectorPresented = true
        case .closeInspector:
            state.selection = .none
            isInspectorPresented = false
        case let .createTicket(title, notes):
            guard state.connection == .connected else { return }
            let payload = TicketCreatePayload(title: title, notes: notes, status: .pitched)
            Task { await harmony.createTicket(payload) }
            isNewTicketSheetPresented = false
        case let .updateTicket(ticketID, patch):
            guard state.connection == .connected else { return }
            state.pendingOps.insert(.ticket(ticketID))
            Task { await harmony.updateTicket(ticketID: ticketID, patch: patch) }
        case let .dispatchRun(ticketID, roleID, model):
            guard state.connection == .connected else { return }
            state.pendingOps.insert(.ticket(ticketID))
            Task { await harmony.dispatchRun(ticketID: ticketID, roleID: roleID, model: model) }
        case let .cancelRun(runID):
            guard state.connection == .connected else { return }
            state.pendingOps.insert(.run(runID))
            Task { await harmony.cancelRun(runID) }
        case let .setProjectMode(projectID, mode):
            guard state.connection == .connected else { return }
            state.pendingOps.insert(.project(projectID))
            Task { await harmony.setProjectMode(projectID: projectID, mode: mode) }
        case .refreshRuntimes:
            Task { await harmony.refreshRuntimes() }
        case let .toggleMode(mode):
            if state.collapsedModeIDs.contains(mode) { state.collapsedModeIDs.remove(mode) }
            else { state.collapsedModeIDs.insert(mode) }
        case .toggleRail:
            state.isRailMode.toggle()
        case let .toggleBlockedFilter(enabled):
            state.showBlocked = enabled
        case let .toggleArchivedFilter(enabled):
            state.showArchived = enabled
        case .showNewTicketSheet:
            isNewTicketSheetPresented = true
        case .showProvidersRolesSheet:
            isProvidersRolesSheetPresented = true
            send(.refreshRuntimes)
        case .hideProvidersRolesSheet:
            isProvidersRolesSheetPresented = false
        case let .selectProvider(id):
            selectedProviderID = id
        case let .showDaemonConfig(config):
            daemonDraft = DaemonDraft(config: config)
            isDaemonConfigPresented = true
        case .addDaemon:
            daemonDraft = DaemonDraft()
            isDaemonConfigPresented = true
        case let .saveDaemon(draft):
            let config = draft.config()
            if let index = state.daemonConfigs.firstIndex(where: { $0.id == config.id }) {
                state.daemonConfigs[index] = config
            } else {
                state.daemonConfigs.append(config)
            }
            state.activeDaemonID = config.id
            isDaemonConfigPresented = false
            state.connection = .connecting
            Task { await harmony.connect(to: config, send: sendFromNetwork(_:)) }
        case let .deleteDaemon(id):
            state.daemonConfigs.removeAll { $0.id == id }
            if state.activeDaemonID == id {
                state.activeDaemonID = state.daemonConfigs.first?.id
            }
            isDaemonConfigPresented = false
        case let .switchDaemon(id):
            guard state.activeDaemonID != id else { return }
            if hasVisibleInProgressRun {
                pendingDaemonSwitchID = id
                isSwitchWarningPresented = true
            } else {
                performDaemonSwitch(id)
            }
        case .confirmDaemonSwitch:
            if let pendingDaemonSwitchID {
                performDaemonSwitch(pendingDaemonSwitchID)
            }
            pendingDaemonSwitchID = nil
            isSwitchWarningPresented = false
        case let .dismissToast(id):
            state.toasts.removeAll { $0.id == id }
            toastTasks[id]?.cancel()
            toastTasks[id] = nil
        case let .viewReport(ticketID, runID):
            send(.selectRun(ticketID, runID))
        }
    }

    private func sendFromNetwork(_ action: AppAction) {
        send(action)
    }

    private func scheduleReconnect() {
        let delay = state.retryDelay
        state.retryDelay = min(delay * 2, 30)
        Task {
            try? await Task.sleep(for: .seconds(delay))
            await harmony.connect(to: state.activeDaemon, send: sendFromNetwork(_:))
        }
    }

    private func addToast(_ toast: Toast, timeout: TimeInterval) {
        state.toasts.append(toast)
        if state.toasts.count > 4 {
            state.toasts.removeFirst(state.toasts.count - 4)
        }
        toastTasks[toast.id]?.cancel()
        toastTasks[toast.id] = Task { [weak self] in
            try? await Task.sleep(for: .seconds(timeout))
            self?.send(.dismissToast(toast.id))
        }
    }

    private func normalizedSelection(_ selection: Selection) -> Selection {
        switch selection {
        case .none:
            .none
        case let .ticket(id):
            state.tickets[id] == nil ? .none : selection
        case let .run(ticketID, _):
            state.tickets[ticketID] == nil ? .none : selection
        }
    }

    private func upsertRun(_ run: Run) {
        guard var ticket = state.tickets[run.ticketID] else { return }
        ticket.activeRunID = run.id
        if let index = ticket.runs.firstIndex(where: { $0.id == run.id }) {
            ticket.runs[index] = run
        } else {
            ticket.runs.append(run)
        }
        state.tickets[ticket.id] = ticket
    }

    private func patchRun(_ runID: RunID, mutate: (inout Run) -> Void) {
        guard let ticketID = ticketID(for: runID), var ticket = state.tickets[ticketID],
              let index = ticket.runs.firstIndex(where: { $0.id == runID }) else { return }
        mutate(&ticket.runs[index])
        if ticket.runs[index].finishedAt != nil {
            ticket.activeRunID = nil
        }
        state.tickets[ticket.id] = ticket
    }

    private func ticketID(for runID: RunID) -> TicketID? {
        state.tickets.values.first { ticket in
            ticket.runs.contains { $0.id == runID }
        }?.id
    }

    private var hasVisibleInProgressRun: Bool {
        state.tickets.values.contains { $0.status == .building || $0.activeRunID != nil }
    }

    private func performDaemonSwitch(_ id: String) {
        state.activeDaemonID = id
        state.tickets.removeAll()
        state.selection = .none
        state.connection = .connecting
        Task { await harmony.connect(to: state.activeDaemon, send: sendFromNetwork(_:)) }
    }
}

enum AppAction: Sendable {
    case appStarted
    case harmonyConnected
    case harmonyDisconnected(String)
    case harmonyError(String)
    case projectsReceived([Project])
    case projectChanged(Project)
    case runtimesReceived([Provider], [Role])
    case ticketChanged(Ticket)
    case runStarted(Run)
    case runProgress(RunID, VoiceEvent)
    case runFinished(RunID, RunReport)
    case runNeedsInput(TicketID, [InputQuestion])
    case wipWarning(TicketStatus, Int)
    case inboxBlocked(Bool)
    case selectProject(ProjectID)
    case selectTicket(TicketID)
    case selectRun(TicketID, RunID)
    case closeInspector
    case createTicket(title: String, notes: String)
    case updateTicket(TicketID, TicketPatch)
    case dispatchRun(TicketID, RoleID, model: String?)
    case cancelRun(RunID)
    case setProjectMode(ProjectID, ProjectMode)
    case refreshRuntimes
    case toggleMode(ProjectMode)
    case toggleRail
    case toggleBlockedFilter(Bool)
    case toggleArchivedFilter(Bool)
    case showNewTicketSheet
    case showProvidersRolesSheet
    case hideProvidersRolesSheet
    case selectProvider(ProviderID)
    case showDaemonConfig(DaemonConfig?)
    case addDaemon
    case saveDaemon(DaemonDraft)
    case deleteDaemon(String)
    case switchDaemon(String)
    case confirmDaemonSwitch
    case dismissToast(UUID)
    case viewReport(TicketID, RunID)
}

struct TicketPatch: Codable, Sendable, Equatable {
    var status: TicketStatus?
    var assignee: String?
    var notes: String?
    var clarifications: [String: String]?
}

struct TicketCreatePayload: Codable, Sendable, Equatable {
    var title: String
    var notes: String
    var status: TicketStatus
}

struct DaemonDraft: Sendable, Equatable {
    var id: String = UUID().uuidString
    var name: String = ""
    var url: String = "ws://localhost:4242/socket"
    var token: String = ""

    init() {}

    init(config: DaemonConfig?) {
        guard let config else { return }
        id = config.id
        name = config.name
        url = config.url.absoluteString
        token = config.token
    }

    func config() -> DaemonConfig {
        DaemonConfig(id: id, name: name.isEmpty ? "Harmony" : name, url: URL(string: url) ?? URL(string: "ws://localhost:4242/socket")!, token: token)
    }
}
