defmodule Harmony.ProjectSubtreeSupervisorTest do
  @moduledoc "Task 1.4: project-registry/start path and crash isolation."
  use ExUnit.Case, async: false

  import Harmony.TestHelpers

  setup do
    # Each test gets its own Registry so process names don't collide
    {:ok, _} = start_supervised({Registry, keys: :unique, name: Harmony.Registry})
    dir_a = make_git_repo()
    dir_b = make_git_repo()

    on_exit(fn ->
      File.rm_rf!(dir_a)
      File.rm_rf!(dir_b)
    end)

    {:ok, dir_a: dir_a, dir_b: dir_b}
  end

  test "starts TicketCache and Dispatcher for a project", %{dir_a: dir_a} do
    project_id = "proj-#{System.unique_integer([:positive])}"
    {:ok, _sup} = start_supervised({Harmony.ProjectSubtreeSupervisor, {project_id, dir_a}})

    assert [{_pid, _}] = Registry.lookup(Harmony.Registry, {:ticket_cache, project_id})
    assert [{_pid, _}] = Registry.lookup(Harmony.Registry, {:dispatcher, project_id})
    assert [{_pid, _}] = Registry.lookup(Harmony.Registry, {:commit_queue, project_id})
  end

  test "TicketCache crash does not restart Dispatcher (one_for_one isolation)", %{dir_a: dir_a} do
    project_id = "proj-#{System.unique_integer([:positive])}"
    {:ok, _sup} = start_supervised({Harmony.ProjectSubtreeSupervisor, {project_id, dir_a}})

    [{dispatcher_pid, _}] = Registry.lookup(Harmony.Registry, {:dispatcher, project_id})
    [{cache_pid, _}] = Registry.lookup(Harmony.Registry, {:ticket_cache, project_id})

    # Kill the TicketCache — it should restart; Dispatcher should survive
    ref = Process.monitor(cache_pid)
    Process.exit(cache_pid, :kill)
    assert_receive {:DOWN, ^ref, :process, ^cache_pid, :killed}, 1000

    # Allow supervisor to restart the cache
    Process.sleep(100)

    # Dispatcher must still be alive with the same pid
    assert Process.alive?(dispatcher_pid),
           "Dispatcher should not have been restarted by TicketCache crash"

    # Cache must have been restarted under a new pid
    [{new_cache_pid, _}] = Registry.lookup(Harmony.Registry, {:ticket_cache, project_id})
    assert new_cache_pid != cache_pid
  end

  test "stopping one project subtree does not affect another", %{dir_a: dir_a, dir_b: dir_b} do
    id_a = "proj-a-#{System.unique_integer([:positive])}"
    id_b = "proj-b-#{System.unique_integer([:positive])}"

    {:ok, _sup_a} =
      start_supervised({Harmony.ProjectSubtreeSupervisor, {id_a, dir_a}},
        id: :sup_a
      )

    {:ok, _sup_b} =
      start_supervised({Harmony.ProjectSubtreeSupervisor, {id_b, dir_b}},
        id: :sup_b
      )

    [{dispatcher_b, _}] = Registry.lookup(Harmony.Registry, {:dispatcher, id_b})

    # Stop project A's subtree
    stop_supervised(:sup_a)

    # Project B's Dispatcher must still be running
    assert Process.alive?(dispatcher_b)

    assert [] == Registry.lookup(Harmony.Registry, {:dispatcher, id_a}),
           "Project A processes should have been cleaned up"
  end
end
