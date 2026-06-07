import SwiftUI

enum StatusTheme {
    static func color(for status: TicketStatus) -> Color {
        Color(status.assetName)
    }

    static func symbol(for status: TicketStatus) -> String {
        switch status {
        case .pitched: "lightbulb"
        case .specced: "doc.text"
        case .ready: "checkmark.circle"
        case .building: "hammer"
        case .awaitingInput: "questionmark.bubble"
        case .reviewing: "eye"
        case .done: "checkmark.seal"
        case .blocked: "exclamationmark.octagon"
        case .archived: "archivebox"
        }
    }

    static func color(for mode: ProjectMode) -> Color {
        Color(mode.assetName)
    }

    static func symbol(for mode: ProjectMode) -> String {
        switch mode {
        case .hot: "flame.fill"
        case .warm: "sun.max.fill"
        case .cold: "snowflake"
        case .maintenance: "wrench.and.screwdriver.fill"
        case .frozen: "pause.circle.fill"
        }
    }
}
