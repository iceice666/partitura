defmodule Harmony.RoleManifest do
  @moduledoc """
  Resolves a role name into a score.role-manifest/v1 JSON file by layering
  the global skill catalog (harmony/skills/<name>/SKILL.md) with per-project
  .score/ overrides (repo wins).
  """

  @default_model %{"provider" => "openai", "id" => "gpt-5-codex"}

  @doc "Resolve a role manifest map, with repo .score/roles/<role>.json overriding globals."
  @spec resolve(String.t(), String.t(), keyword()) :: map()
  def resolve(role, repo_path, opts \\ []) do
    dispatch_mode = Keyword.get(opts, :dispatch_mode, "independent")
    skill_name = Keyword.get(opts, :skill, role)

    global = %{
      "schema" => "score.role-manifest/v1",
      "role" => role,
      "dispatch_mode" => dispatch_mode,
      "system_prompt" => "You are the #{role} role for Partitura.",
      "skill" => skill(skill_name),
      "model" => @default_model,
      "tools" => %{"mcp_servers" => [], "allow" => []},
      "budgets" => %{"max_turns" => 60, "max_tokens" => 2_000_000, "max_seconds" => 3_600}
    }

    deep_merge(global, repo_override(repo_path, role))
  end

  @doc "Resolve and write a role manifest JSON file under .score/runs."
  @spec write!(String.t(), String.t(), String.t(), keyword()) :: String.t()
  def write!(role, repo_path, run_id, opts \\ []) do
    manifest = resolve(role, repo_path, opts)
    dir = Path.join([repo_path, ".score", "runs", "_manifests"])
    File.mkdir_p!(dir)
    path = Path.join(dir, "#{run_id}-#{role}.json")
    File.write!(path, Jason.encode!(manifest, pretty: true))
    path
  end

  defp skill(skill_name) do
    path = Path.expand(Path.join([__DIR__, "..", "..", "skills", skill_name, "SKILL.md"]))

    if File.exists?(path) do
      %{"name" => skill_name, "body" => strip_frontmatter(File.read!(path))}
    else
      %{"name" => skill_name, "body" => ""}
    end
  end

  defp strip_frontmatter("---\n" <> rest) do
    case String.split(rest, "\n---\n", parts: 2) do
      [_frontmatter, body] -> body
      _ -> rest
    end
  end

  defp strip_frontmatter(body), do: body

  defp repo_override(repo_path, role) do
    path = Path.join([repo_path, ".score", "roles", "#{role}.json"])

    with {:ok, content} <- File.read(path),
         {:ok, data} when is_map(data) <- Jason.decode(content) do
      data
    else
      _ -> %{}
    end
  end

  defp deep_merge(left, right) when is_map(left) and is_map(right) do
    Map.merge(left, right, fn _key, lval, rval ->
      if is_map(lval) and is_map(rval), do: deep_merge(lval, rval), else: rval
    end)
  end
end
