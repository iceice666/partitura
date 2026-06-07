import SwiftUI

enum Theme {
    struct Spacing {
        let xs: CGFloat = 4
        let sm: CGFloat = 8
        let md: CGFloat = 12
        let lg: CGFloat = 16
        let xl: CGFloat = 24
    }

    struct Radius {
        let card: CGFloat = 8
        let button: CGFloat = 6
        let badge: CGFloat = 5
        let toast: CGFloat = 8
    }

    static let spacing = Spacing()
    static let radius = Radius()
    static let columnWidth: CGFloat = 296
    static let cardWidth: CGFloat = 296
    static let sidebarWidth: CGFloat = 240
    static let sidebarRailWidth: CGFloat = 56
    static let inspectorWidth: CGFloat = 420
    static let toastWidth: CGFloat = 320
    static let providersSheetSize = CGSize(width: 560, height: 480)
}

enum ThemeColors {
    static let surfaceBase = Color("SurfaceBase")
    static let surfaceRaised = Color("SurfaceRaised")
    static let sidebarMaterial = Color("SidebarMaterial")
    static let cardFill = Color("CardFill")
    static let cardBorder = Color("CardBorder")
    static let textPrimary = Color("TextPrimary")
    static let textSecondary = Color("TextSecondary")
    static let textTertiary = Color("TextTertiary")
    static let warning = Color(red: 0.82, green: 0.47, blue: 0.04)
    static let awaitingTint = Color(red: 0.97, green: 0.20, blue: 0.55).opacity(0.14)
}
