defmodule Harmony.Recovery do
  @moduledoc """
  Restart recovery orchestration run at project-subtree start (D9).

  Rebuilds cache from git HEAD, resets building→ready, recomputes WIP,
  rebuilds the dispatch queue. Human-pending states (reviewing, awaiting_input)
  are left untouched.
  """

  @doc "Run restart recovery for a project subtree."
  @spec run(String.t(), String.t()) :: :ok | {:error, term()}
  def run(project_id, repo_path) do
    try do
      with :ok <- Harmony.TicketCache.rebuild(project_id),
           :ok <- reset_building_tickets(project_id, repo_path) do
        Harmony.TicketCache.rebuild(project_id)
      end
    catch
      :exit, reason -> {:error, reason}
    end
  end

  defp reset_building_tickets(project_id, repo_path) do
    project_id
    |> Harmony.TicketCache.snapshot()
    |> Enum.filter(&(Map.get(&1, "status") == "building"))
    |> Enum.reduce_while(:ok, fn ticket, :ok ->
      ticket_id = ticket["id"]

      result =
        Harmony.Git.patch_ticket(
          project_id,
          repo_path,
          ticket_id,
          %{"status" => "ready", "branch" => nil, "started_at" => nil},
          "score: reset #{ticket_id} building->ready on daemon restart"
        )

      case result do
        :ok ->
          File.rm_rf!(Path.join([repo_path, ".score", "workspaces", ticket_id]))
          {:cont, :ok}

        {:error, reason} ->
          {:halt, {:error, reason}}
      end
    end)
  end
end
