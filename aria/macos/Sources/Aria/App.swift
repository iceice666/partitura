import SwiftUI

@main
struct AriaApp: App {
    @State private var store = AppStore.preview()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(store)
                .frame(minWidth: 1180, minHeight: 720)
        }
        .windowStyle(.automatic)
        .commands {
            CommandGroup(after: .newItem) {
                Button("New Ticket") { store.send(.showNewTicketSheet) }
                    .keyboardShortcut("n", modifiers: [.command])
                Button("Providers/Roles") { store.send(.showProvidersRolesSheet) }
                    .keyboardShortcut("r", modifiers: [.command, .shift])
            }
        }
    }
}

struct RootView: View {
    @Environment(AppStore.self) private var store
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    var body: some View {
        @Bindable var store = store

        NavigationSplitView {
            SidebarView()
                .navigationSplitViewColumnWidth(
                    min: Theme.sidebarRailWidth,
                    ideal: store.state.isRailMode ? Theme.sidebarRailWidth : Theme.sidebarWidth,
                    max: Theme.sidebarWidth
                )
        } detail: {
            BoardView()
                .inspector(isPresented: $store.isInspectorPresented) {
                    TicketInspectorView()
                        .inspectorColumnWidth(Theme.inspectorWidth)
                }
                .overlay(alignment: .bottomTrailing) {
                    ToastStack()
                        .padding(Theme.spacing.lg)
                }
                .sheet(isPresented: $store.isNewTicketSheetPresented) {
                    NewTicketSheet()
                }
                .sheet(isPresented: $store.isProvidersRolesSheetPresented) {
                    ProvidersRolesSheet()
                }
        }
        .background(reduceTransparency ? Color("SurfaceBase") : Color.clear)
        .task {
            store.send(.appStarted)
        }
    }
}
