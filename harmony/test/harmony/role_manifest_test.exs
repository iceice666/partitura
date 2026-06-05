defmodule Harmony.RoleManifestTest do
  use ExUnit.Case, async: false

  import Harmony.TestHelpers

  setup do
    repo = make_git_repo()
    on_exit(fn -> File.rm_rf!(repo) end)
    {:ok, repo: repo}
  end

  test "repo role override wins over global defaults", %{repo: repo} do
    dir = Path.join([repo, ".score", "roles"])
    File.mkdir_p!(dir)

    File.write!(
      Path.join(dir, "builder.json"),
      Jason.encode!(%{"model" => %{"provider" => "stub", "id" => "repo-model"}})
    )

    manifest = Harmony.RoleManifest.resolve("builder", repo, skill: "spec")

    assert manifest["schema"] == "score.role-manifest/v1"
    assert manifest["role"] == "builder"
    assert manifest["model"] == %{"provider" => "stub", "id" => "repo-model"}
    assert manifest["skill"]["name"] == "spec"
    assert String.contains?(manifest["skill"]["body"], "spec")
  end

  test "write! stores manifest JSON and returns its path", %{repo: repo} do
    path = Harmony.RoleManifest.write!("builder", repo, "run-1", skill: "spec")

    assert File.exists?(path)
    assert {:ok, %{"schema" => "score.role-manifest/v1"}} = Jason.decode(File.read!(path))
  end
end
