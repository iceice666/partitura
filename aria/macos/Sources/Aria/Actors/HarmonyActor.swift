import Foundation
import SwiftPhoenixClient

actor HarmonyActor {
    private var socket: Socket?
    private var lobbyChannel: Channel?
    private var projectChannel: Channel?
    private var activeProjectID: ProjectID?
    private var reconnectGeneration = 0
    private var sendToStore: (@MainActor @Sendable (AppAction) -> Void)?

    func connect(to daemon: DaemonConfig?, send: @escaping @MainActor @Sendable (AppAction) -> Void) async {
        sendToStore = send
        reconnectGeneration += 1
        socket?.disconnect()

        guard let daemon else {
            await emitFixtureSnapshot(send: send)
            return
        }

        if daemon.token == "local-dev" {
            await emitFixtureSnapshot(send: send)
            return
        }

        let newSocket = Socket(daemon.url.absoluteString, params: ["token": daemon.token])
        newSocket.reconnectAfter = { tries in min(pow(2, Double(tries)), 30) }
        newSocket.logger = { line in
            fputs("[HarmonyActor] \(line)\n", stderr)
        }
        newSocket.onOpen {
            Task { await self.handleSocketOpen() }
        }
        newSocket.onClose { _, reason in
            Task { await self.dispatch(.harmonyDisconnected(reason ?? "Connection closed")) }
        }
        newSocket.onError { error, _ in
            Task { await self.dispatch(.harmonyError(error.localizedDescription)) }
        }
        socket = newSocket
        newSocket.connect()
    }

    func joinProject(_ id: ProjectID) async {
        activeProjectID = id
        projectChannel?.leave()
        guard let socket else { return }
        let channel = socket.channel("project:\(id)")
        bindProject(channel)
        projectChannel = channel
        channel.join()
        channel.push("ticket:list", payload: ["project_id": id])
    }

    func createTicket(_ payload: TicketCreatePayload) async {
        projectChannel?.push("ticket:create", payload: encodePayload(payload))
    }

    func updateTicket(ticketID: TicketID, patch: TicketPatch) async {
        projectChannel?.push("ticket:update", payload: ["id": ticketID, "patch": encodePayload(patch)])
    }

    func dispatchRun(ticketID: TicketID, roleID: RoleID, model: String?) async {
        var payload: Payload = ["ticket_id": ticketID, "role": roleID]
        if let model { payload["model"] = model }
        projectChannel?.push("run:dispatch", payload: payload)
    }

    func cancelRun(_ runID: RunID) async {
        projectChannel?.push("run:cancel", payload: ["run_id": runID])
    }

    func setProjectMode(projectID: ProjectID, mode: ProjectMode) async {
        lobbyChannel?.push("project:set_mode", payload: ["project_id": projectID, "mode": mode.rawValue])
    }

    func refreshRuntimes() async {
        lobbyChannel?.push("runtimes:snapshot", payload: [:])
    }

    private func handleSocketOpen() async {
        await dispatch(.harmonyConnected)
        guard let socket else { return }
        let lobby = socket.channel("projects:lobby")
        bindLobby(lobby)
        lobbyChannel = lobby
        lobby.join()
        lobby.push("runtimes:snapshot", payload: [:])
        if let activeProjectID {
            await joinProject(activeProjectID)
        }
    }

    private func bindLobby(_ channel: Channel) {
        channel.on("projects:list") { message in
            Task { await self.handleProjectsList(message.payload) }
        }
        channel.on("runtimes:snapshot") { message in
            Task { await self.handleRuntimeSnapshot(message.payload) }
        }
        channel.on("project:changed") { message in
            Task { await self.handleProjectChanged(message.payload) }
        }
    }

    private func bindProject(_ channel: Channel) {
        channel.on("ticket:changed") { message in
            Task { await self.decodeAndDispatch(Ticket.self, payload: message.payload) { AppAction.ticketChanged($0) } }
        }
        channel.on("run:started") { message in
            Task { await self.decodeAndDispatch(Run.self, payload: message.payload) { AppAction.runStarted($0) } }
        }
        channel.on("run:progress") { message in
            Task { await self.handleRunProgress(message.payload) }
        }
        channel.on("run:finished") { message in
            Task { await self.handleRunFinished(message.payload) }
        }
        channel.on("run:needs_input") { message in
            Task { await self.handleNeedsInput(message.payload) }
        }
        channel.on("wip:warning") { message in
            Task { await self.handleWipWarning(message.payload) }
        }
        channel.on("inbox:blocked") { message in
            Task { await self.dispatch(.inboxBlocked((message.payload["blocked"] as? Bool) ?? true)) }
        }
    }

    private func emitFixtureSnapshot(send: @escaping @MainActor @Sendable (AppAction) -> Void) async {
        await MainActor.run {
            send(.harmonyConnected)
            send(.projectsReceived(PreviewFixtures.projects))
            send(.runtimesReceived(PreviewFixtures.providers, PreviewFixtures.roles))
            for ticket in PreviewFixtures.tickets {
                send(.ticketChanged(ticket))
            }
        }
    }

    private func dispatch(_ action: AppAction) async {
        guard let sendToStore else { return }
        await MainActor.run { sendToStore(action) }
    }

    private func decodeAndDispatch<T: Decodable>(_ type: T.Type, payload: Payload, as map: @escaping @Sendable (T) -> AppAction) async {
        do {
            let value = try decode(type, from: payload)
            await dispatch(map(value))
        } catch {
            await dispatch(.harmonyError("Failed to decode Harmony payload: \(error.localizedDescription)"))
        }
    }

    private func handleRuntimeSnapshot(_ payload: Payload) async {
        do {
            let providers = try decode([Provider].self, fromJSONObject: payload["providers"] ?? [])
            let roles = try decode([Role].self, fromJSONObject: payload["roles"] ?? [])
            await dispatch(.runtimesReceived(providers, roles))
        } catch {
            await dispatch(.harmonyError("Failed to decode runtimes snapshot: \(error.localizedDescription)"))
        }
    }

    private func handleProjectsList(_ payload: Payload) async {
        do {
            let rawProjects = payload["projects"] ?? payload
            let projects = try decode([Project].self, fromJSONObject: rawProjects)
            await dispatch(.projectsReceived(projects))
        } catch {
            await dispatch(.harmonyError("Failed to decode project list: \(error.localizedDescription)"))
        }
    }

    private func handleProjectChanged(_ payload: Payload) async {
        do {
            let project = try decode(Project.self, from: payload)
            await dispatch(.projectChanged(project))
        } catch {
            await dispatch(.harmonyError("Failed to decode project update: \(error.localizedDescription)"))
        }
    }

    private func handleRunProgress(_ payload: Payload) async {
        guard let runID = payload["run_id"] as? String else { return }
        let eventPayload = (payload["event"] as? Payload) ?? payload
        let event = VoiceEvent(type: eventPayload["t"] as? String ?? eventPayload["type"] as? String ?? "text", text: eventPayload["delta"] as? String ?? eventPayload["msg"] as? String ?? "")
        await dispatch(.runProgress(runID, event))
    }

    private func handleRunFinished(_ payload: Payload) async {
        guard let runID = payload["run_id"] as? String else { return }
        do {
            let report = try decode(RunReport.self, from: payload)
            await dispatch(.runFinished(runID, report))
        } catch {
            await dispatch(.harmonyError("Failed to decode run report: \(error.localizedDescription)"))
        }
    }

    private func handleNeedsInput(_ payload: Payload) async {
        guard let ticketID = payload["ticket_id"] as? String else { return }
        do {
            let questions = try decode([InputQuestion].self, fromJSONObject: payload["questions"] ?? [])
            await dispatch(.runNeedsInput(ticketID, questions))
        } catch {
            await dispatch(.harmonyError("Failed to decode input questions: \(error.localizedDescription)"))
        }
    }

    private func handleWipWarning(_ payload: Payload) async {
        guard let raw = payload["column"] as? String, let status = TicketStatus(rawValue: raw) else { return }
        await dispatch(.wipWarning(status, payload["count"] as? Int ?? 0))
    }

    private func decode<T: Decodable>(_ type: T.Type, from payload: Payload) throws -> T {
        try decode(type, fromJSONObject: payload)
    }

    private func decode<T: Decodable>(_ type: T.Type, fromJSONObject object: Any) throws -> T {
        let data = try JSONSerialization.data(withJSONObject: object)
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode(type, from: data)
    }

    private func encodePayload<T: Encodable>(_ value: T) -> Payload {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        encoder.dateEncodingStrategy = .iso8601
        guard let data = try? encoder.encode(value),
              let payload = try? JSONSerialization.jsonObject(with: data) as? Payload else {
            return [:]
        }
        return payload
    }
}
