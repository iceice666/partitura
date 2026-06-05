defmodule HarmonyWeb.LobbyChannel do
  @moduledoc """
  projects:lobby — project list and health.
  """
  use Phoenix.Channel

  @impl true
  def join("projects:lobby", _params, socket) do
    {:ok, socket}
  end

  @impl true
  def handle_in("projects:list", _payload, socket) do
    projects =
      Enum.map(Harmony.Config.registered_projects(), fn project ->
        %{
          "id" => project.id,
          "name" => Map.get(project, :name, project.id),
          "mode" => project.mode,
          "counts" => counts(project.id)
        }
      end)

    {:reply, {:ok, %{"projects" => projects}}, socket}
  end

  def handle_in(_event, _payload, socket) do
    {:reply, {:error, %{"reason" => "unknown_event"}}, socket}
  end

  defp counts(project_id) do
    try do
      Harmony.TicketCache.counts(project_id)
    catch
      :exit, _ -> %{}
    end
  end
end
