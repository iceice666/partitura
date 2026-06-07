import AppKit
import SwiftUI

struct SidebarView: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                if !store.state.isRailMode {
                    Text("Projects")
                        .font(.headline)
                    Spacer()
                }
                Button {
                    store.send(.toggleRail)
                } label: {
                    Image(systemName: store.state.isRailMode ? "sidebar.left" : "sidebar.leading")
                }
                .buttonStyle(.borderless)
                .accessibilityLabel(store.state.isRailMode ? "Expand sidebar" : "Collapse sidebar")
                .help(store.state.isRailMode ? "Expand sidebar" : "Collapse sidebar")
            }
            .padding(Theme.spacing.md)

            ScrollView {
                LazyVStack(alignment: .leading, spacing: Theme.spacing.sm) {
                    ForEach(ProjectMode.allCases) { mode in
                        let projects = store.state.projects.filter { $0.mode == mode }
                        if !projects.isEmpty {
                            ProjectModeGroup(mode: mode, projects: projects)
                        }
                    }
                }
                .padding(.horizontal, Theme.spacing.sm)
            }

            Spacer(minLength: 0)
            DaemonStrip()
        }
        .background(ThemeColors.sidebarMaterial)
    }
}

struct ProjectModeGroup: View {
    @Environment(AppStore.self) private var store
    let mode: ProjectMode
    let projects: [Project]

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.spacing.xs) {
            if !store.state.isRailMode {
                Button {
                    store.send(.toggleMode(mode))
                } label: {
                    HStack {
                        Text(mode.title.uppercased())
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(ThemeColors.textSecondary)
                        Spacer()
                        Image(systemName: store.state.collapsedModeIDs.contains(mode) ? "chevron.right" : "chevron.down")
                            .font(.caption)
                    }
                }
                .buttonStyle(.plain)
                .accessibilityLabel("\(mode.title) project group")
            }

            if !store.state.collapsedModeIDs.contains(mode) || store.state.isRailMode {
                ForEach(projects) { project in
                    ProjectRow(project: project)
                        .draggable(project.id)
                }
            }
        }
        .dropDestination(for: String.self) { items, _ in
            guard let id = items.first else { return false }
            store.send(.setProjectMode(id, mode))
            return true
        }
        .padding(.vertical, Theme.spacing.xs)
    }
}

struct ProjectRow: View {
    @Environment(AppStore.self) private var store
    let project: Project

    private var isSelected: Bool { store.state.activeProjectID == project.id }
    private var isPending: Bool { store.state.pendingOps.contains(.project(project.id)) }

    var body: some View {
        Button {
            store.send(.selectProject(project.id))
        } label: {
            HStack(spacing: Theme.spacing.sm) {
                Circle()
                    .fill(StatusTheme.color(for: project.mode))
                    .frame(width: 9, height: 9)
                    .accessibilityLabel("\(project.mode.title) mode")
                if !store.state.isRailMode {
                    Text(project.name)
                        .lineLimit(1)
                    Spacer()
                    Text("\(project.ticketCount)")
                        .font(.caption2.weight(.semibold))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(.quaternary, in: Capsule())
                }
            }
            .frame(maxWidth: .infinity, minHeight: 28, alignment: store.state.isRailMode ? .center : .leading)
            .padding(.horizontal, Theme.spacing.sm)
            .background(isSelected ? Color.accentColor.opacity(0.18) : Color.clear, in: RoundedRectangle(cornerRadius: Theme.radius.button))
            .overlay {
                if isPending {
                    RoundedRectangle(cornerRadius: Theme.radius.button)
                        .stroke(StatusTheme.color(for: project.mode), lineWidth: 1)
                }
            }
        }
        .buttonStyle(.plain)
        .opacity(isPending ? 0.65 : 1)
        .help(project.name)
        .accessibilityLabel("\(project.name), \(project.mode.title), \(project.ticketCount) tickets")
        .pointer()
    }
}

struct DaemonStrip: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        @Bindable var store = store
        HStack(spacing: Theme.spacing.sm) {
            DaemonIndicator(state: store.state.connection)
            if !store.state.isRailMode {
                Button(store.state.activeDaemon?.name ?? "No daemon") {
                    store.isDaemonSelectorPresented = true
                }
                .buttonStyle(.plain)
                .lineLimit(1)
                .popover(isPresented: $store.isDaemonSelectorPresented) {
                    DaemonSelectorPopover()
                }
                Spacer()
                Button {
                    store.send(.showDaemonConfig(store.state.activeDaemon))
                } label: {
                    Image(systemName: "gearshape")
                }
                .buttonStyle(.borderless)
                .accessibilityLabel("Configure daemon")
                .help("Configure daemon")
                .popover(isPresented: $store.isDaemonConfigPresented) {
                    DaemonConfigPopover()
                }
            }
        }
        .padding(Theme.spacing.md)
        .accessibilityLabel("Daemon \(store.state.connection.rawValue)")
    }
}

struct DaemonIndicator: View {
    let state: ConnectionState
    @State private var spin = false

    var body: some View {
        ZStack {
            switch state {
            case .connected:
                Circle().fill(.green)
            case .connecting:
                Circle().stroke(.yellow, lineWidth: 2)
                Circle()
                    .trim(from: 0, to: 0.25)
                    .stroke(.yellow, lineWidth: 2)
                    .rotationEffect(.degrees(spin ? 360 : 0))
                    .onAppear { withAnimation(.linear(duration: 0.8).repeatForever(autoreverses: false)) { spin = true } }
            case .disconnected:
                Circle().stroke(.red, lineWidth: 2)
            }
        }
        .frame(width: 12, height: 12)
        .accessibilityLabel("Daemon \(state.rawValue)")
    }
}

struct DaemonSelectorPopover: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        @Bindable var store = store
        VStack(alignment: .leading, spacing: Theme.spacing.sm) {
            ForEach(store.state.daemonConfigs) { daemon in
                Button {
                    store.send(.switchDaemon(daemon.id))
                } label: {
                    HStack {
                        Image(systemName: store.state.activeDaemonID == daemon.id ? "checkmark.circle.fill" : "circle")
                        Text(daemon.name)
                        Spacer()
                    }
                }
                .buttonStyle(.plain)
            }
            Divider()
            Button {
                store.send(.addDaemon)
            } label: {
                Label("Add daemon", systemImage: "plus")
            }
        }
        .padding()
        .frame(width: 260)
        .alert("Switch daemon while a run is in progress?", isPresented: $store.isSwitchWarningPresented) {
            Button("Cancel", role: .cancel) {
                store.pendingDaemonSwitchID = nil
            }
            Button("Switch") {
                store.send(.confirmDaemonSwitch)
            }
            .disabled(store.pendingDaemonSwitchID == nil)
        } message: {
            Text("Switching clears the visible board and connects to the selected daemon.")
        }
    }
}

struct DaemonConfigPopover: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        @Bindable var store = store
        Form {
            TextField("Name", text: $store.daemonDraft.name)
            TextField("URL", text: $store.daemonDraft.url)
            SecureField("Token", text: $store.daemonDraft.token)
            HStack {
                Button("Delete", role: .destructive) {
                    store.send(.deleteDaemon(store.daemonDraft.id))
                }
                Spacer()
                Button("Save") {
                    store.send(.saveDaemon(store.daemonDraft))
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 320)
    }
}

struct BoardView: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        VStack(spacing: 0) {
            BoardToolbar()
            if store.state.connection != .connected {
                ReconnectBanner()
            }
            if store.state.inboxBlocked {
                InboxFullBanner()
            }
            ScrollView(.horizontal) {
                HStack(alignment: .top, spacing: Theme.spacing.lg) {
                    ForEach(TicketStatus.primaryColumns) { status in
                        ColumnView(status: status, tickets: store.state.tickets(for: status))
                    }
                    if store.state.showBlocked || store.state.showArchived {
                        ColumnView(status: .blocked, title: "Other", tickets: store.state.otherTickets())
                    }
                }
                .padding(Theme.spacing.lg)
            }
            .background(ThemeColors.surfaceBase)
        }
    }
}

struct BoardToolbar: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        HStack(spacing: Theme.spacing.sm) {
            Text(store.state.activeProject?.name ?? "Board")
                .font(.title3.weight(.semibold))
            Spacer()
            Toggle("Blocked", isOn: Binding(get: { store.state.showBlocked }, set: { store.send(.toggleBlockedFilter($0)) }))
                .toggleStyle(.button)
            Toggle("Archived", isOn: Binding(get: { store.state.showArchived }, set: { store.send(.toggleArchivedFilter($0)) }))
                .toggleStyle(.button)
            Button {
                store.send(.showProvidersRolesSheet)
            } label: {
                Label("Providers/Roles", systemImage: "person.2.badge.gearshape")
            }
            Button {
                store.send(.showNewTicketSheet)
            } label: {
                Label("New ticket", systemImage: "plus")
            }
            .disabled(store.state.isReadOnly)
        }
        .padding(.horizontal, Theme.spacing.lg)
        .padding(.vertical, Theme.spacing.md)
        .background(.bar)
    }
}

struct ColumnView: View {
    @Environment(AppStore.self) private var store
    let status: TicketStatus
    var title: String?
    let tickets: [Ticket]

    private var warning: Int? { store.state.wipWarnings[status] }

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.spacing.md) {
            HStack {
                Text(title ?? status.title)
                    .font(.system(size: 12, weight: .semibold))
                    .textCase(.uppercase)
                Spacer()
                Text("[\(tickets.count)/\(wipLimitText)]")
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 3)
                    .background(warning == nil ? Color.secondary.opacity(0.12) : ThemeColors.warning.opacity(0.18), in: Capsule())
            }
            .foregroundStyle(warning == nil ? ThemeColors.textSecondary : ThemeColors.warning)

            LazyVStack(spacing: Theme.spacing.md) {
                ForEach(tickets) { ticket in
                    TicketCardView(ticket: ticket)
                }
            }
            Spacer(minLength: 0)
        }
        .frame(width: Theme.columnWidth)
        .padding(Theme.spacing.md)
        .background(status == .awaitingInput ? ThemeColors.awaitingTint : ThemeColors.surfaceRaised, in: RoundedRectangle(cornerRadius: Theme.radius.card))
        .overlay(RoundedRectangle(cornerRadius: Theme.radius.card).stroke(Color("CardBorder")))
    }

    private var wipLimitText: String {
        switch status {
        case .ready, .building: "4"
        case .reviewing: "2"
        default: "-"
        }
    }
}

struct TicketCardView: View {
    @Environment(AppStore.self) private var store
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hovering = false
    @State private var pulse = false
    let ticket: Ticket

    private var isPending: Bool { store.state.pendingOps.contains(.ticket(ticket.id)) }

    var body: some View {
        Button {
            store.send(.selectTicket(ticket.id))
        } label: {
            VStack(alignment: .leading, spacing: Theme.spacing.sm) {
                HStack {
                    StatusBadge(status: ticket.status)
                    Spacer()
                    if ticket.isInfeasibleReturn {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(.red)
                            .accessibilityLabel("Infeasible return")
                    }
                    if ticket.status == .building {
                        Circle()
                            .fill(StatusTheme.color(for: .building))
                            .frame(width: 8, height: 8)
                            .opacity(reduceMotion ? 1 : (pulse ? 0.35 : 1))
                            .onAppear { withAnimation(Motion.pulse(reduceMotion: reduceMotion)) { pulse.toggle() } }
                            .accessibilityLabel("Active run")
                    }
                }
                Text(ticket.title)
                    .font(.headline)
                    .foregroundStyle(ThemeColors.textPrimary)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                HStack {
                    Label(ticket.assignee ?? "Unassigned", systemImage: "person.crop.circle")
                    Spacer()
                    Text(ticket.created, style: .relative)
                }
                .font(.caption)
                .foregroundStyle(ThemeColors.textSecondary)
                FlowTags(tags: ticket.tags)
            }
            .padding(Theme.spacing.md)
            .frame(width: Theme.cardWidth, alignment: .leading)
            .background(ThemeColors.cardFill, in: RoundedRectangle(cornerRadius: Theme.radius.card))
            .overlay(RoundedRectangle(cornerRadius: Theme.radius.card).stroke(hovering ? Color.accentColor : Color("CardBorder")))
            .overlay {
                if isPending {
                    ShimmerOverlay()
                }
            }
        }
        .buttonStyle(.plain)
        .disabled(isPending)
        .opacity(isPending ? 0.62 : 1)
        .onHover { hovering = $0 }
        .pointer()
        .accessibilityLabel("\(ticket.title), \(ticket.status.title), \(ticket.assignee ?? "unassigned")")
    }
}

struct FlowTags: View {
    let tags: [String]
    var body: some View {
        HStack {
            ForEach(tags.prefix(3), id: \.self) { tag in
                Text(tag)
                    .font(.caption2.weight(.medium))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 3)
                    .background(.quaternary, in: Capsule())
            }
        }
    }
}

struct StatusBadge: View {
    let status: TicketStatus
    var body: some View {
        Label(status.title, systemImage: StatusTheme.symbol(for: status))
            .font(.caption.weight(.semibold))
            .padding(.horizontal, 7)
            .padding(.vertical, 4)
            .foregroundStyle(.white)
            .background(StatusTheme.color(for: status), in: RoundedRectangle(cornerRadius: Theme.radius.badge))
            .accessibilityLabel(status.title)
    }
}

struct ShimmerOverlay: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var offset: CGFloat = -180

    var body: some View {
        Rectangle()
            .fill(.linearGradient(colors: [.clear, .white.opacity(0.38), .clear], startPoint: .leading, endPoint: .trailing))
            .offset(x: reduceMotion ? 0 : offset)
            .clipShape(RoundedRectangle(cornerRadius: Theme.radius.card))
            .onAppear {
                withAnimation(Motion.shimmer(reduceMotion: reduceMotion)) {
                    offset = 180
                }
            }
            .allowsHitTesting(false)
    }
}

struct ReconnectBanner: View {
    @Environment(AppStore.self) private var store
    var body: some View {
        HStack {
            Image(systemName: "wifi.exclamationmark")
            Text("Reconnecting in \(Int(store.state.retryDelay))s")
            Text(store.state.connectionReason ?? "Board is read-only until Harmony reconnects.")
                .foregroundStyle(.secondary)
            Spacer()
        }
        .padding(Theme.spacing.md)
        .background(.yellow.opacity(0.18))
        .accessibilityLabel("Reconnect banner. Board is read only.")
    }
}

struct InboxFullBanner: View {
    @Environment(AppStore.self) private var store
    var body: some View {
        HStack {
            Image(systemName: "tray.full.fill")
            Text("Inbox is blocked")
            Spacer()
            Button("Show waiting") {
                store.send(.toggleBlockedFilter(true))
            }
        }
        .padding(Theme.spacing.md)
        .background(Color.pink.opacity(0.14))
    }
}

struct NewTicketSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var notes = ""

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.spacing.md) {
            Text("New Ticket")
                .font(.title2.weight(.semibold))
            TextField("Title", text: $title)
            TextEditor(text: $notes)
                .frame(height: 140)
                .overlay(RoundedRectangle(cornerRadius: Theme.radius.card).stroke(Color("CardBorder")))
            StatusBadge(status: .pitched)
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Create") {
                    store.send(.createTicket(title: title, notes: notes))
                }
                .disabled(title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || store.state.isReadOnly)
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(Theme.spacing.xl)
        .frame(width: 420)
    }
}

struct ProvidersRolesSheet: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        HStack(spacing: 0) {
            List(selection: Binding(get: { store.selectedProviderID }, set: { if let id = $0 { store.send(.selectProvider(id)) } })) {
                ForEach(store.state.providers) { provider in
                    HStack {
                        Image(systemName: provider.hasCredentials ? "checkmark.circle.fill" : "xmark.circle")
                            .foregroundStyle(provider.hasCredentials ? .green : .red)
                        VStack(alignment: .leading) {
                            Text(provider.name)
                            Text(provider.hasCredentials ? "Credentials available" : "Missing credentials")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        if !provider.hasCredentials {
                            Button("Configure") {}
                        }
                    }
                    .tag(Optional(provider.id))
                }
            }
            .frame(width: 230)

            VStack(alignment: .leading) {
                Text("Roles")
                    .font(.headline)
                ForEach(filteredRoles) { role in
                    let available = store.state.roleAvailability(role)
                    HStack {
                        VStack(alignment: .leading) {
                            Text(role.name)
                            Text("\(role.providerID) / \(role.model)")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Image(systemName: available ? "checkmark.circle" : "lock")
                    }
                    .foregroundStyle(available ? ThemeColors.textPrimary : ThemeColors.textTertiary)
                    .padding(.vertical, Theme.spacing.xs)
                    .accessibilityLabel("\(role.name), \(available ? "available" : "unavailable")")
                }
                Spacer()
                HStack {
                    Spacer()
                    Button("Done") { store.send(.hideProvidersRolesSheet) }
                        .keyboardShortcut(.defaultAction)
                }
            }
            .padding(Theme.spacing.lg)
        }
        .frame(width: Theme.providersSheetSize.width, height: Theme.providersSheetSize.height)
    }

    private var filteredRoles: [Role] {
        guard let selected = store.selectedProviderID else { return store.state.roles }
        return store.state.roles.filter { $0.providerID == selected }
    }
}

struct TicketInspectorView: View {
    @Environment(AppStore.self) private var store
    @State private var showReject = false

    var body: some View {
        if let ticket = store.state.selectedTicket {
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: Theme.spacing.lg) {
                        InspectorHeader(ticket: ticket)
                        AssigneePicker(ticket: ticket)
                        MarkdownSection(title: "Spec", text: ticket.spec?.what ?? "No spec yet.")
                        MarkdownListSection(title: "Acceptance", items: ticket.spec?.acceptance ?? [])
                        if let pitch = ticket.pitch {
                            MarkdownSection(title: "Pitch", text: "\(pitch.appetite): \(pitch.text)")
                        }
                        BlockersSection(ticket: ticket)
                        NeedsInputForm(ticket: ticket)
                        RunHistoryList(ticket: ticket)
                        InspectorActions(ticket: ticket, showReject: $showReject)
                    }
                    .padding(Theme.spacing.lg)
                }
                .onChange(of: store.focusedRunID) { _, runID in
                    if let runID {
                        withAnimation { proxy.scrollTo(runID, anchor: .top) }
                    }
                }
            }
            .frame(width: Theme.inspectorWidth)
            .sheet(isPresented: $showReject) {
                RejectNotesSheet(ticket: ticket)
            }
            .onExitCommand {
                store.send(.closeInspector)
            }
        } else {
            ContentUnavailableView("No Ticket", systemImage: "rectangle.stack")
                .frame(width: Theme.inspectorWidth)
        }
    }
}

struct InspectorHeader: View {
    @Environment(AppStore.self) private var store
    let ticket: Ticket

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.spacing.sm) {
            HStack {
                Text(ticket.id)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                Spacer()
                Button {
                    store.send(.closeInspector)
                } label: {
                    Image(systemName: "xmark")
                }
                .accessibilityLabel("Close inspector")
            }
            Text(ticket.title)
                .font(.title2.weight(.semibold))
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                StatusBadge(status: ticket.status)
                FlowTags(tags: ticket.tags)
            }
        }
    }
}

struct AssigneePicker: View {
    @Environment(AppStore.self) private var store
    let ticket: Ticket
    @State private var modelOverride: String?

    var body: some View {
        Menu {
            Button("@me") {
                store.send(.updateTicket(ticket.id, TicketPatch(status: nil, assignee: "@me", notes: nil, clarifications: nil)))
            }
            ForEach(store.state.roles) { role in
                let available = store.state.roleAvailability(role)
                Menu("@\(role.name) \(available ? "" : "(unavailable)")") {
                    Button("Default: \(role.providerID) / \(role.model)") {
                        modelOverride = nil
                        store.send(.updateTicket(ticket.id, TicketPatch(status: nil, assignee: "@\(role.name)", notes: nil, clarifications: nil)))
                    }
                    ForEach(store.state.provider(for: role.providerID)?.models ?? [], id: \.self) { model in
                        Button(model) {
                            modelOverride = model
                            store.send(.updateTicket(ticket.id, TicketPatch(status: nil, assignee: "@\(role.name)", notes: nil, clarifications: nil)))
                        }
                    }
                }
                .disabled(!available)
            }
        } label: {
            Label(ticket.assignee ?? "Unassigned", systemImage: "person.crop.circle")
        }
        .disabled(store.state.isReadOnly)
        .accessibilityLabel("Assignee picker")
    }
}

struct MarkdownSection: View {
    let title: String
    let text: String
    var body: some View {
        VStack(alignment: .leading, spacing: Theme.spacing.sm) {
            Text(title).font(.headline)
            Text(text).textSelection(.enabled)
        }
    }
}

struct MarkdownListSection: View {
    let title: String
    let items: [String]
    var body: some View {
        VStack(alignment: .leading, spacing: Theme.spacing.sm) {
            Text(title).font(.headline)
            ForEach(items, id: \.self) { item in
                Label(item, systemImage: "checkmark.circle")
            }
        }
    }
}

struct BlockersSection: View {
    @Environment(AppStore.self) private var store
    let ticket: Ticket

    var body: some View {
        if !ticket.blockedBy.isEmpty {
            VStack(alignment: .leading, spacing: Theme.spacing.sm) {
                Text("Blockers").font(.headline)
                ForEach(ticket.blockedBy, id: \.self) { id in
                    HStack {
                        Text(id)
                        Spacer()
                        if let blocker = store.state.tickets[id] {
                            StatusBadge(status: blocker.status)
                        }
                    }
                }
            }
        }
    }
}

struct NeedsInputForm: View {
    @Environment(AppStore.self) private var store
    let ticket: Ticket
    @State private var answers: [String: String] = [:]

    var body: some View {
        if !ticket.needsInput.isEmpty {
            VStack(alignment: .leading, spacing: Theme.spacing.sm) {
                Text("Needs Input").font(.headline)
                ForEach(ticket.needsInput) { question in
                    switch question.kind {
                    case .text:
                        TextField(question.label, text: binding(for: question.id))
                    case .secure:
                        SecureField(question.label, text: binding(for: question.id))
                    case .picker:
                        Picker(question.label, selection: binding(for: question.id)) {
                            ForEach(question.options, id: \.self) { Text($0).tag($0) }
                        }
                    }
                }
                Button("Submit clarification") {
                    store.send(.updateTicket(ticket.id, TicketPatch(status: .ready, assignee: nil, notes: nil, clarifications: answers)))
                }
                .disabled(store.state.isReadOnly || missingRequired)
            }
            .padding(Theme.spacing.md)
            .background(Color.pink.opacity(0.12), in: RoundedRectangle(cornerRadius: Theme.radius.card))
        }
    }

    private var missingRequired: Bool {
        ticket.needsInput.contains { $0.required && (answers[$0.id] ?? "").isEmpty }
    }

    private func binding(for id: String) -> Binding<String> {
        Binding(get: { answers[id, default: ""] }, set: { answers[id] = $0 })
    }
}

struct RunHistoryList: View {
    let ticket: Ticket
    var body: some View {
        VStack(alignment: .leading, spacing: Theme.spacing.sm) {
            Text("Runs").font(.headline)
            ForEach(ticket.runs.sorted { $0.startedAt > $1.startedAt }) { run in
                RunEntryView(run: run)
                    .id(run.id)
            }
        }
    }
}

struct RunEntryView: View {
    @State private var expanded: Bool
    let run: Run

    init(run: Run) {
        self.run = run
        _expanded = State(initialValue: run.finishedAt == nil)
    }

    var body: some View {
        DisclosureGroup(isExpanded: $expanded) {
            LiveLogView(run: run)
            if let report = run.report {
                VStack(alignment: .leading, spacing: Theme.spacing.sm) {
                    Text(report.summary)
                    MarkdownListSection(title: "Files changed", items: report.filesChanged)
                    MarkdownListSection(title: "Acceptance", items: report.acceptance)
                    if let infeasibility = report.infeasibility {
                        InfeasibleView(infeasibility: infeasibility)
                    }
                    if let notes = report.notes {
                        Text(notes).font(.callout)
                    }
                }
                .padding(.top, Theme.spacing.sm)
            }
        } label: {
            HStack {
                Text(run.id).font(.caption.monospaced())
                Spacer()
                Text(run.report?.exitReason.rawValue ?? "running")
                    .font(.caption.weight(.semibold))
            }
            .padding(.vertical, 4)
        }
        .padding(Theme.spacing.md)
        .background(run.report?.exitReason == .infeasible ? Color.red.opacity(0.12) : ThemeColors.surfaceRaised, in: RoundedRectangle(cornerRadius: Theme.radius.card))
        .onChange(of: run.finishedAt) { _, finishedAt in
            if finishedAt != nil {
                expanded = false
            }
        }
    }
}

struct LiveLogView: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var tailing = true
    @State private var scrollPosition: VoiceEvent.ID?
    let run: Run

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.spacing.xs) {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: Theme.spacing.xs) {
                    ForEach(run.progress) { event in
                        Text("[\(event.type)] \(event.text)")
                            .font(.system(.caption, design: .monospaced))
                            .foregroundStyle(ThemeColors.textSecondary)
                            .id(event.id)
                    }
                }
            }
            .frame(maxHeight: 180)
            .scrollPosition(id: $scrollPosition, anchor: .bottom)
            .onScrollPhaseChange { _, phase in
                if phase.isScrolling {
                    tailing = false
                }
            }
            if !tailing {
                Button("Resume tailing") { tailing = true }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(Theme.spacing.sm)
        .background(.black.opacity(0.08), in: RoundedRectangle(cornerRadius: Theme.radius.card))
        .onChange(of: run.progress.last?.id) { _, lastID in
            guard tailing else { return }
            scrollPosition = lastID
        }
        .onChange(of: tailing) { _, isTailing in
            if isTailing {
                scrollPosition = run.progress.last?.id
            }
        }
        .onAppear {
            scrollPosition = run.progress.last?.id
        }
        .animation(Motion.standard(reduceMotion: reduceMotion), value: run.progress.count)
    }
}

struct InfeasibleView: View {
    let infeasibility: Infeasibility
    var body: some View {
        VStack(alignment: .leading, spacing: Theme.spacing.sm) {
            Text("Infeasible").font(.headline).foregroundStyle(.red)
            Text(infeasibility.reason)
            MarkdownListSection(title: "Prerequisites", items: infeasibility.prerequisites)
            MarkdownListSection(title: "Suggested changes", items: infeasibility.suggestedChanges)
        }
        .padding(Theme.spacing.md)
        .background(Color.red.opacity(0.12), in: RoundedRectangle(cornerRadius: Theme.radius.card))
    }
}

struct InspectorActions: View {
    @Environment(AppStore.self) private var store
    let ticket: Ticket
    @Binding var showReject: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.spacing.sm) {
            Text("Actions").font(.headline)
            HStack {
                Button("Dispatch") {
                    if let role = store.state.roles.first(where: { store.state.roleAvailability($0) }) {
                        store.send(.dispatchRun(ticket.id, role.id, model: nil))
                    }
                }
                .disabled(store.state.isReadOnly || ticket.status != .ready)
                Button("Cancel") {
                    if let runID = ticket.activeRunID { store.send(.cancelRun(runID)) }
                }
                .disabled(store.state.isReadOnly || ticket.activeRunID == nil)
                Button("Approve") {
                    store.send(.updateTicket(ticket.id, TicketPatch(status: .done, assignee: nil, notes: nil, clarifications: nil)))
                }
                .disabled(store.state.isReadOnly || ticket.status != .reviewing)
                Button("Reject", role: .destructive) { showReject = true }
                    .disabled(store.state.isReadOnly || ticket.status != .reviewing)
            }
            HStack {
                Button("Move to Ready") {
                    store.send(.updateTicket(ticket.id, TicketPatch(status: .ready, assignee: nil, notes: nil, clarifications: nil)))
                }
                .disabled(store.state.isReadOnly || ticket.status != .specced || ticket.spec == nil)
                Button("Mark Blocked") {
                    store.send(.updateTicket(ticket.id, TicketPatch(status: .blocked, assignee: nil, notes: "Blocked from inspector", clarifications: nil)))
                }
                .disabled(store.state.isReadOnly)
                Button("Unblock") {
                    store.send(.updateTicket(ticket.id, TicketPatch(status: .ready, assignee: nil, notes: nil, clarifications: nil)))
                }
                .disabled(store.state.isReadOnly || ticket.status != .blocked)
            }
        }
    }
}

struct RejectNotesSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let ticket: Ticket
    @State private var notes = ""

    var body: some View {
        VStack(alignment: .leading, spacing: Theme.spacing.md) {
            Text("Reject with Notes").font(.title2.weight(.semibold))
            TextEditor(text: $notes)
                .frame(height: 160)
                .overlay(RoundedRectangle(cornerRadius: Theme.radius.card).stroke(Color("CardBorder")))
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Reject", role: .destructive) {
                    store.send(.updateTicket(ticket.id, TicketPatch(status: .ready, assignee: nil, notes: notes, clarifications: nil)))
                    dismiss()
                }
                .disabled(notes.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(Theme.spacing.xl)
        .frame(width: 420)
    }
}

struct ToastStack: View {
    @Environment(AppStore.self) private var store
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    var body: some View {
        VStack(alignment: .trailing, spacing: Theme.spacing.sm) {
            ForEach(store.state.toasts.suffix(4)) { toast in
                HStack(alignment: .top, spacing: Theme.spacing.sm) {
                    if case let .runFinished(_, _, failed) = toast.kind, failed {
                        Rectangle().fill(.red).frame(width: 4)
                    }
                    Image(systemName: toast.symbol)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(toast.title).font(.headline)
                        Text(toast.body).font(.caption).lineLimit(3)
                        if case let .runFinished(ticketID, runID, _) = toast.kind {
                            Button("View report") {
                                store.send(.viewReport(ticketID, runID))
                            }
                            .buttonStyle(.link)
                        }
                    }
                    Spacer()
                    Button {
                        store.send(.dismissToast(toast.id))
                    } label: {
                        Image(systemName: "xmark")
                    }
                    .buttonStyle(.borderless)
                    .accessibilityLabel("Dismiss notification")
                }
                .padding(Theme.spacing.md)
                .frame(width: Theme.toastWidth)
                .background(reduceTransparency ? ThemeColors.surfaceRaised : Color.clear)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: Theme.radius.toast))
                .transition(.move(edge: .trailing).combined(with: .opacity))
            }
        }
        .animation(Motion.toast(reduceMotion: reduceMotion), value: store.state.toasts)
    }
}

extension View {
    func pointer() -> some View {
        onHover { inside in
            if inside { NSCursor.pointingHand.push() }
            else { NSCursor.pop() }
        }
    }
}
