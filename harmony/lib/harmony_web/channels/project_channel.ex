defmodule HarmonyWeb.ProjectChannel do
  @moduledoc """
  project:<project_id> — ticket and run events for one project.
  """
  use Phoenix.Channel

  @impl true
  def join("project:" <> project_id, _params, socket) do
    socket = assign(socket, :project_id, project_id)
    {:ok, %{"tickets" => Harmony.TicketCache.snapshot(project_id)}, socket}
  end

  @impl true
  def handle_in("ticket:list", _payload, socket) do
    {:reply, {:ok, %{"tickets" => Harmony.TicketCache.snapshot(socket.assigns.project_id)}},
     socket}
  end

  def handle_in("ticket:create", ticket, socket) when is_map(ticket) do
    project_id = socket.assigns.project_id

    with %{repo_path: repo_path} <- project(project_id),
         :ok <-
           Harmony.Git.create_ticket(
             project_id,
             repo_path,
             ticket,
             "score: #{ticket["id"]} create"
           ),
         {:ok, content} <-
           Harmony.Git.show_head_file(repo_path, ".score/tickets/#{ticket["id"]}.yaml"),
         :ok <- Harmony.TicketCache.update_from_content(project_id, content) do
      changed = Harmony.TicketCache.get(project_id, ticket["id"])
      broadcast!(socket, "ticket:changed", changed)
      {:reply, {:ok, changed}, socket}
    else
      error -> {:reply, {:error, %{"reason" => inspect(error)}}, socket}
    end
  end

  def handle_in("ticket:update", %{"id" => ticket_id, "patch" => patch}, socket)
      when is_map(patch) do
    project_id = socket.assigns.project_id

    with %{repo_path: repo_path} <- project(project_id),
         current when is_map(current) <- Harmony.TicketCache.get(project_id, ticket_id),
         :ok <- validate_status_transition(current, patch),
         :ok <-
           Harmony.Git.patch_ticket(
             project_id,
             repo_path,
             ticket_id,
             patch,
             "score: #{ticket_id} update"
           ),
         {:ok, content} <-
           Harmony.Git.show_head_file(repo_path, ".score/tickets/#{ticket_id}.yaml"),
         :ok <- Harmony.TicketCache.update_from_content(project_id, content) do
      changed = Harmony.TicketCache.get(project_id, ticket_id)
      broadcast!(socket, "ticket:changed", changed)
      {:reply, {:ok, changed}, socket}
    else
      nil -> {:reply, {:error, %{"reason" => "ticket_not_found"}}, socket}
      {:error, reason} -> {:reply, {:error, %{"reason" => format_error(reason)}}, socket}
      error -> {:reply, {:error, %{"reason" => inspect(error)}}, socket}
    end
  end

  def handle_in("run:dispatch", %{"ticket_id" => ticket_id, "role" => role} = payload, socket) do
    opts =
      payload
      |> Map.take(["model"])
      |> Enum.map(fn {key, value} -> {String.to_atom(key), value} end)

    case Harmony.Dispatcher.dispatch(socket.assigns.project_id, ticket_id, role, opts) do
      {:ok, run} ->
        {:reply, {:ok, run}, socket}

      {:error, {:inbox_full, message}} ->
        broadcast!(socket, "inbox:blocked", %{"message" => message})
        {:reply, {:error, %{"reason" => message}}, socket}

      {:error, reason} ->
        {:reply, {:error, %{"reason" => inspect(reason)}}, socket}
    end
  end

  def handle_in("run:cancel", %{"run_id" => run_id}, socket) do
    case Harmony.Dispatcher.cancel(socket.assigns.project_id, run_id) do
      :ok -> {:reply, :ok, socket}
      {:error, reason} -> {:reply, {:error, %{"reason" => inspect(reason)}}, socket}
    end
  end

  def handle_in(_event, _payload, socket) do
    {:reply, {:error, %{"reason" => "unknown_event"}}, socket}
  end

  # Validate a status transition guard before committing.
  # Merges any spec patch into the ticket preview so answer+status updates work together.
  defp validate_status_transition(_current, patch) when not is_map_key(patch, "status"), do: :ok

  defp validate_status_transition(current, %{"status" => target} = patch) do
    preview =
      case Map.get(patch, "spec") do
        nil ->
          current

        spec_patch ->
          existing = Map.get(current, "spec", %{})
          Map.put(current, "spec", Map.merge(existing, spec_patch))
      end

    Harmony.StateMachine.validate_transition(preview, target)
  end

  defp format_error(:missing_spec), do: "ready requires a spec field"

  defp format_error(:unanswered_questions),
    do: "awaiting_input->ready requires all questions answered"

  defp format_error({:invalid_status, s}), do: "invalid status: #{s}"
  defp format_error(reason), do: inspect(reason)

  defp project(project_id) do
    Enum.find(Harmony.Config.registered_projects(), &(&1.id == project_id))
  end
end
