import SwiftUI

#Preview("Density 1440x900") {
    RootView()
        .environment(AppStore.preview())
        .frame(width: 1440, height: 900)
}

#Preview("Dark Status Colors") {
    VStack(alignment: .leading, spacing: Theme.spacing.md) {
        ForEach(TicketStatus.allCases) { status in
            StatusBadge(status: status)
        }
        Divider()
        HStack {
            ForEach(ProjectMode.allCases) { mode in
                Label(mode.title, systemImage: StatusTheme.symbol(for: mode))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 5)
                    .background(StatusTheme.color(for: mode), in: RoundedRectangle(cornerRadius: Theme.radius.badge))
            }
        }
    }
    .padding()
    .background(ThemeColors.surfaceBase)
    .preferredColorScheme(.dark)
}

#Preview("Reduced Motion and Transparency") {
    RootView()
        .environment(AppStore.preview())
        .frame(width: 1440, height: 900)
}
