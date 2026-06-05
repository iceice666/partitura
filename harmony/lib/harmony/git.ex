defmodule Harmony.Git do
  @moduledoc """
  Git I/O primitives for Harmony.

  All reads shell out to git via System.cmd. All writes go through
  Harmony.Git.CommitQueue to serialise per-project commits and prevent
  index races (D5). The public write API therefore takes both project_id
  (for the queue lookup) and repo_path (for the actual git operations).

  Ticket writes are field-preserving: only Harmony-managed fields are
  updated; human-owned fields (notes, pitch, tags, spec.what, etc.) are
  never clobbered (D5).
  """

  # Harmony-managed ticket top-level fields
  @harmony_top_fields ~w(status branch started_at last_run_id completed_at)

  # Harmony-managed spec sub-fields (append semantics for list fields)
  @harmony_spec_fields ~w(rework_notes respec_notes clarifications handoff_notes)

  # ── Read primitives ──────────────────────────────────────────────────────────

  @doc "Read a file path from git HEAD in the given repo."
  @spec show_head_file(String.t(), String.t()) :: {:ok, String.t()} | {:error, term()}
  def show_head_file(repo_path, sub_path) do
    case System.cmd("git", ["show", "HEAD:#{sub_path}"],
           cd: repo_path,
           stderr_to_stdout: true
         ) do
      {content, 0} -> {:ok, content}
      {_, _} -> {:error, :not_found}
    end
  end

  @doc "Read a file at a specific commit SHA."
  @spec show_file_at(String.t(), String.t(), String.t()) :: {:ok, String.t()} | {:error, term()}
  def show_file_at(repo_path, sha, sub_path) do
    case System.cmd("git", ["show", "#{sha}:#{sub_path}"],
           cd: repo_path,
           stderr_to_stdout: true
         ) do
      {content, 0} -> {:ok, content}
      {_, _} -> {:error, :not_found}
    end
  end

  @doc """
  Return the list of file paths changed in a commit (all paths; filter in caller).
  """
  @spec diff_tree_files(String.t(), String.t()) :: {:ok, [String.t()]} | {:error, term()}
  def diff_tree_files(repo_path, sha) do
    case System.cmd("git", ["diff-tree", "--name-only", "-r", sha],
           cd: repo_path,
           stderr_to_stdout: false
         ) do
      {output, 0} ->
        files =
          output
          |> String.split("\n", trim: true)
          |> Enum.reject(&(&1 == sha or &1 == ""))

        {:ok, files}

      {_, _} ->
        {:error, :git_failed}
    end
  end

  @doc "List filenames under a tree path at HEAD (e.g. .score/tickets/)."
  @spec ls_tree_names(String.t(), String.t()) :: {:ok, [String.t()]} | {:error, term()}
  def ls_tree_names(repo_path, sub_path) do
    case System.cmd("git", ["ls-tree", "--name-only", "HEAD", sub_path],
           cd: repo_path,
           stderr_to_stdout: false
         ) do
      {output, 0} ->
        names =
          output
          |> String.split("\n", trim: true)
          |> Enum.map(&Path.basename/1)

        {:ok, names}

      {_, _} ->
        {:error, :not_found}
    end
  end

  @doc "Install Harmony post-commit and post-merge hooks in a project repo."
  @spec install_hooks(String.t(), String.t()) :: :ok | {:error, term()}
  def install_hooks(repo_path, harmony_command \\ "harmony") do
    hooks_dir = Path.join([repo_path, ".git", "hooks"])
    File.mkdir_p!(hooks_dir)

    content = """
    #!/bin/sh
    # post hook installed by `harmony register` - do not remove
    #{harmony_command} notify --repo="$(pwd)" --commit="$(git rev-parse HEAD)"
    """

    Enum.reduce_while(~w(post-commit post-merge), :ok, fn hook, :ok ->
      path = Path.join(hooks_dir, hook)

      with :ok <- File.write(path, content),
           :ok <- File.chmod(path, 0o755) do
        {:cont, :ok}
      else
        {:error, reason} -> {:halt, {:error, {hook, reason}}}
      end
    end)
  end

  # ── Identity resolution ──────────────────────────────────────────────────────

  @doc """
  Resolve the git author identity for this repo.
  Searches: <repo>/.git/config → ~/.gitconfig → fallback (harmony <harmony@localhost>).
  """
  @spec git_identity(String.t()) :: %{name: String.t(), email: String.t()}
  def git_identity(repo_path) do
    name = git_config_get(repo_path, "user.name") || "harmony"
    email = git_config_get(repo_path, "user.email") || "harmony@localhost"
    %{name: name, email: email}
  end

  defp git_config_get(repo_path, key) do
    case System.cmd("git", ["config", key], cd: repo_path, stderr_to_stdout: false) do
      {value, 0} -> String.trim(value)
      _ -> nil
    end
  end

  # ── Field-preserving ticket writes ──────────────────────────────────────────

  @doc """
  Patch a ticket YAML file, preserving all human-owned fields.

  Only fields in @harmony_top_fields and @harmony_spec_fields are updated.
  List fields in spec (rework_notes, respec_notes, clarifications) are
  APPENDED, not replaced. Serialised through CommitQueue (D5).

  Returns :ok or {:error, reason}.
  """
  @spec patch_ticket(String.t(), String.t(), String.t(), map(), String.t()) ::
          :ok | {:error, term()}
  def patch_ticket(project_id, repo_path, ticket_id, patch, commit_message) do
    Harmony.Git.CommitQueue.commit(project_id, fn ->
      do_patch_ticket(repo_path, ticket_id, patch, commit_message)
    end)
  end

  defp do_patch_ticket(repo_path, ticket_id, patch, commit_message) do
    file_rel = ".score/tickets/#{ticket_id}.yaml"
    file_abs = Path.join(repo_path, file_rel)

    current =
      case show_head_file(repo_path, file_rel) do
        {:ok, content} ->
          case YamlElixir.read_from_string(content) do
            {:ok, map} when is_map(map) -> map
            _ -> %{}
          end

        {:error, _} ->
          %{}
      end

    # Separate the patch into top-level fields and spec sub-fields
    {top_patch, spec_patch} = split_patch(patch)

    # Merge top-level Harmony-managed fields only
    merged_top = Map.merge(current, Map.take(top_patch, @harmony_top_fields))

    merged =
      if map_size(spec_patch) > 0 do
        existing_spec = Map.get(merged_top, "spec", %{})
        new_spec = merge_spec(existing_spec, spec_patch)
        Map.put(merged_top, "spec", new_spec)
      else
        merged_top
      end

    case Ymlr.document(merged) do
      {:ok, new_content} ->
        File.mkdir_p!(Path.dirname(file_abs))
        File.write!(file_abs, new_content)
        commit_file(repo_path, file_rel, commit_message)

      {:error, reason} ->
        {:error, {:yaml_encode_failed, reason}}
    end
  end

  @doc "Write a new ticket file and commit it. Serialised through CommitQueue."
  @spec create_ticket(String.t(), String.t(), map(), String.t()) :: :ok | {:error, term()}
  def create_ticket(project_id, repo_path, ticket_map, commit_message) do
    Harmony.Git.CommitQueue.commit(project_id, fn ->
      file_rel = ".score/tickets/#{Map.fetch!(ticket_map, "id")}.yaml"
      file_abs = Path.join(repo_path, file_rel)

      case Ymlr.document(ticket_map) do
        {:ok, content} ->
          File.mkdir_p!(Path.dirname(file_abs))
          File.write!(file_abs, content)
          commit_file(repo_path, file_rel, commit_message)

        {:error, reason} ->
          {:error, {:yaml_encode_failed, reason}}
      end
    end)
  end

  @doc "Parse ticket YAML content into a map."
  @spec parse_ticket(String.t()) :: {:ok, map()} | {:error, term()}
  def parse_ticket(content) do
    case YamlElixir.read_from_string(content) do
      {:ok, map} when is_map(map) -> {:ok, map}
      {:ok, _} -> {:error, :not_a_map}
      {:error, reason} -> {:error, reason}
    end
  end

  # ── Private helpers ──────────────────────────────────────────────────────────

  defp commit_file(repo_path, file_rel, message) do
    identity = git_identity(repo_path)

    with {_, 0} <- System.cmd("git", ["add", file_rel], cd: repo_path),
         {_, 0} <-
           System.cmd(
             "git",
             [
               "commit",
               "--author",
               "#{identity.name} <#{identity.email}>",
               "-m",
               message
             ],
             cd: repo_path
           ) do
      :ok
    else
      {output, code} -> {:error, {:git_commit_failed, code, output}}
    end
  end

  defp split_patch(patch) do
    spec_patch = Map.get(patch, "spec", %{})
    top_patch = Map.delete(patch, "spec")
    {top_patch, Map.take(spec_patch, @harmony_spec_fields)}
  end

  defp merge_spec(existing, new_spec_fields) do
    Enum.reduce(new_spec_fields, existing, fn {key, value}, acc ->
      case {Map.get(acc, key), value} do
        {existing_list, new_items}
        when is_list(existing_list) and is_list(new_items) and key == "clarifications" ->
          # Clarifications are merged by "id" so answers update their entry, not append duplicates.
          Map.put(acc, key, merge_clarifications(existing_list, new_items))

        {existing_list, new_items} when is_list(existing_list) and is_list(new_items) ->
          Map.put(acc, key, existing_list ++ new_items)

        _ ->
          Map.put(acc, key, value)
      end
    end)
  end

  defp merge_clarifications(existing, updates) do
    update_index = Map.new(updates, fn entry -> {entry["id"], entry} end)
    existing_ids = MapSet.new(existing, & &1["id"])

    updated =
      Enum.map(existing, fn entry ->
        case Map.get(update_index, entry["id"]) do
          nil -> entry
          patch -> Map.merge(entry, patch)
        end
      end)

    new_entries = Enum.reject(updates, fn entry -> MapSet.member?(existing_ids, entry["id"]) end)
    updated ++ new_entries
  end
end
