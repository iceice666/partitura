defmodule Harmony.TestHelpers do
  @moduledoc "Helpers shared across the test suite."

  @doc """
  Create a temporary directory with a valid git repo.

  Sets up user.name and user.email in the local config and makes an
  initial commit so HEAD is valid. Caller is responsible for cleanup.
  """
  @spec make_git_repo() :: String.t()
  def make_git_repo do
    dir = Path.join(System.tmp_dir!(), "harmony_test_#{System.unique_integer([:positive])}")
    File.mkdir_p!(dir)
    {_, 0} = System.cmd("git", ["init"], cd: dir)
    {_, 0} = System.cmd("git", ["config", "user.email", "test@harmony.local"], cd: dir)
    {_, 0} = System.cmd("git", ["config", "user.name", "Harmony Test"], cd: dir)
    # Initial commit so HEAD ref is valid
    readme = Path.join(dir, "README.md")
    File.write!(readme, "# Test\n")
    {_, 0} = System.cmd("git", ["add", "README.md"], cd: dir)
    {_, 0} = System.cmd("git", ["commit", "-m", "init"], cd: dir)
    dir
  end

  @doc "Write a minimal ticket YAML and commit it to the repo."
  @spec commit_ticket(String.t(), String.t(), map()) :: :ok
  def commit_ticket(repo_path, ticket_id, fields \\ %{}) do
    dir = Path.join(repo_path, ".score/tickets")
    File.mkdir_p!(dir)
    path = Path.join(dir, "#{ticket_id}.yaml")

    status = Map.get(fields, "status", "pitched")
    title = Map.get(fields, "title", "Test ticket #{ticket_id}")

    spec_section =
      case Map.get(fields, "spec") do
        nil -> ""
        spec -> "spec:\n  what: #{Map.get(spec, "what", "test")}\n"
      end

    extra =
      fields
      |> Map.drop(["status", "title", "spec"])
      |> Enum.map(fn {k, v} -> "#{k}: #{inspect_yaml(v)}\n" end)
      |> Enum.join()

    content = """
    schema: score.ticket/v1
    id: #{ticket_id}
    title: "#{title}"
    status: #{status}
    created: "2026-06-05"
    #{extra}#{spec_section}
    """

    File.write!(path, content)
    {_, 0} = System.cmd("git", ["add", ".score/tickets/#{ticket_id}.yaml"], cd: repo_path)
    {_, 0} = System.cmd("git", ["commit", "-m", "add ticket #{ticket_id}"], cd: repo_path)
    :ok
  end

  @doc "Write a YAML config file to a path."
  @spec write_yaml_config(String.t(), map()) :: :ok
  def write_yaml_config(path, data) do
    File.mkdir_p!(Path.dirname(path))
    {:ok, content} = Ymlr.document(data)
    File.write!(path, content)
  end

  defp inspect_yaml(v) when is_binary(v), do: "\"#{v}\""
  defp inspect_yaml(v) when is_boolean(v), do: to_string(v)
  defp inspect_yaml(v) when is_number(v), do: to_string(v)
  defp inspect_yaml(v), do: inspect(v)
end
