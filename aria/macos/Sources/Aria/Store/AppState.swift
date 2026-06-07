import Foundation

struct AppState: Sendable {
    var connection: ConnectionState = .disconnected
    var connectionReason: String?
    var retryDelay: TimeInterval = 1
    var projects: [Project] = []
    var activeProjectID: ProjectID?
    var tickets: [TicketID: Ticket] = [:]
    var selection: Selection = .none
    var providers: [Provider] = []
    var roles: [Role] = []
    var pendingOps: Set<PendingOperation> = []
    var toasts: [Toast] = []
    var daemonConfigs: [DaemonConfig] = []
    var activeDaemonID: String?
    var collapsedModeIDs: Set<ProjectMode> = []
    var isRailMode = false
    var showBlocked = false
    var showArchived = false
    var inboxBlocked = false
    var wipWarnings: [TicketStatus: Int] = [:]

    var activeProject: Project? {
        projects.first { $0.id == activeProjectID }
    }

    var activeDaemon: DaemonConfig? {
        daemonConfigs.first { $0.id == activeDaemonID }
    }

    var isReadOnly: Bool {
        connection != .connected
    }

    var selectedTicket: Ticket? {
        if case let .ticket(id) = selection { tickets[id] }
        else if case let .run(id, _) = selection { tickets[id] }
        else { nil }
    }

    func tickets(for status: TicketStatus) -> [Ticket] {
        tickets.values
            .filter { $0.status == status }
            .sorted { $0.created > $1.created }
    }

    func otherTickets() -> [Ticket] {
        tickets.values
            .filter { ticket in
                (showBlocked && ticket.status == .blocked) || (showArchived && ticket.status == .archived)
            }
            .sorted { $0.created > $1.created }
    }

    func provider(for id: ProviderID) -> Provider? {
        providers.first { $0.id == id }
    }

    func roleAvailability(_ role: Role) -> Bool {
        provider(for: role.providerID)?.hasCredentials == true
    }
}
