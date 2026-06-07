import Foundation

typealias ProjectID = String
typealias TicketID = String
typealias RunID = String
typealias RoleID = String
typealias ProviderID = String

enum ConnectionState: String, Codable, Sendable, CaseIterable {
    case disconnected
    case connecting
    case connected
}

enum ProjectMode: String, Codable, Sendable, CaseIterable, Identifiable {
    case hot
    case warm
    case cold
    case maintenance
    case frozen

    var id: String { rawValue }
    var title: String { rawValue.replacingOccurrences(of: "_", with: " ").capitalized }
    var assetName: String { "Mode" + title.replacingOccurrences(of: " ", with: "") }
}

enum TicketStatus: String, Codable, Sendable, CaseIterable, Identifiable {
    case pitched
    case specced
    case ready
    case building
    case awaitingInput = "awaiting_input"
    case reviewing
    case done
    case blocked
    case archived

    var id: String { rawValue }
    var title: String {
        switch self {
        case .awaitingInput: "Awaiting Input"
        default: rawValue.replacingOccurrences(of: "_", with: " ").capitalized
        }
    }
    var assetName: String {
        switch self {
        case .awaitingInput: "StatusAwaitingInput"
        default: "Status" + title.replacingOccurrences(of: " ", with: "")
        }
    }
    var isPrimaryColumn: Bool { TicketStatus.primaryColumns.contains(self) }
    static let primaryColumns: [TicketStatus] = [.pitched, .specced, .ready, .building, .awaitingInput, .reviewing, .done]
}

enum Selection: Equatable, Sendable {
    case none
    case ticket(TicketID)
    case run(TicketID, RunID)
}

struct Project: Identifiable, Codable, Sendable, Equatable {
    var id: ProjectID
    var name: String
    var mode: ProjectMode
    var ticketCount: Int
}

struct TicketSpec: Codable, Sendable, Equatable {
    var what: String
    var acceptance: [String]
    var reworkNotes: String?
    var respecNotes: String?
    var clarifications: [String: String]
    var handoffNotes: String?
}

struct Pitch: Codable, Sendable, Equatable {
    var appetite: String
    var text: String
}

struct Ticket: Identifiable, Codable, Sendable, Equatable {
    var id: TicketID
    var title: String
    var status: TicketStatus
    var created: Date
    var assignee: String?
    var tags: [String]
    var spec: TicketSpec?
    var pitch: Pitch?
    var blockedBy: [TicketID]
    var runs: [Run]
    var activeRunID: RunID?
    var needsInput: [InputQuestion]

    var isInfeasibleReturn: Bool { spec?.respecNotes?.isEmpty == false }
}

enum RunExitReason: String, Codable, Sendable, CaseIterable {
    case completed
    case failed
    case hardAbort = "hard-abort"
    case infeasible
    case needsInput = "needs-input"
    case cancelled
}

struct Run: Identifiable, Codable, Sendable, Equatable {
    var id: RunID
    var ticketID: TicketID
    var role: RoleID
    var model: String
    var startedAt: Date
    var finishedAt: Date?
    var progress: [VoiceEvent]
    var report: RunReport?
}

struct VoiceEvent: Identifiable, Codable, Sendable, Equatable {
    var id = UUID()
    var type: String
    var text: String
}

struct RunReport: Codable, Sendable, Equatable {
    var exitReason: RunExitReason
    var summary: String
    var filesChanged: [String]
    var acceptance: [String]
    var notes: String?
    var infeasibility: Infeasibility?
}

struct Infeasibility: Codable, Sendable, Equatable {
    var reason: String
    var prerequisites: [String]
    var suggestedChanges: [String]
}

struct Provider: Identifiable, Codable, Sendable, Equatable {
    var id: ProviderID
    var name: String
    var hasCredentials: Bool
    var models: [String]
}

struct Role: Identifiable, Codable, Sendable, Equatable {
    var id: RoleID
    var name: String
    var providerID: ProviderID
    var model: String
}

enum InputQuestionKind: String, Codable, Sendable {
    case text
    case secure
    case picker
}

struct InputQuestion: Identifiable, Codable, Sendable, Equatable {
    var id: String
    var label: String
    var kind: InputQuestionKind
    var required: Bool
    var options: [String]
}

struct DaemonConfig: Identifiable, Codable, Sendable, Equatable {
    var id: String
    var name: String
    var url: URL
    var token: String
}

enum PendingOperation: Hashable, Sendable {
    case ticket(TicketID)
    case project(ProjectID)
    case run(RunID)
}

enum ToastKind: Sendable, Equatable {
    case general
    case daemon
    case runFinished(TicketID, RunID, failed: Bool)
}

struct Toast: Identifiable, Sendable, Equatable {
    var id = UUID()
    var kind: ToastKind
    var title: String
    var body: String
    var symbol: String
}
