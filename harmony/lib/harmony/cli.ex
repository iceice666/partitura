defmodule Harmony.CLI do
  @moduledoc false

  def main(["register", repo_path]) do
    exit_with(Harmony.Git.install_hooks(Path.expand(repo_path)))
  end

  def main(["notify" | args]) do
    {opts, _rest, invalid} = OptionParser.parse(args, strict: [repo: :string, commit: :string])

    case {invalid, opts[:repo], opts[:commit]} do
      {[], repo, commit} when is_binary(repo) and is_binary(commit) ->
        exit_with(Harmony.GitHookReceiver.notify(repo, commit))

      _ ->
        usage()
        System.halt(2)
    end
  end

  def main(_args) do
    usage()
    System.halt(2)
  end

  defp exit_with(:ok), do: :ok

  defp exit_with({:error, reason}) do
    IO.puts(:stderr, "harmony: #{inspect(reason)}")
    System.halt(1)
  end

  defp usage do
    IO.puts(:stderr, """
    usage:
      harmony register <repo>
      harmony notify --repo <repo> --commit <sha>
    """)
  end
end
