defmodule Harmony.MixProject do
  use Mix.Project

  def project do
    [
      app: :harmony,
      version: "0.1.0",
      elixir: "~> 1.16",
      elixirc_paths: elixirc_paths(Mix.env()),
      start_permanent: Mix.env() == :prod,
      escript: [main_module: Harmony.CLI],
      deps: deps()
    ]
  end

  def application do
    [
      mod: {Harmony.Application, []},
      extra_applications: [:logger, :runtime_tools]
    ]
  end

  defp elixirc_paths(:test), do: ["lib", "test/support"]
  defp elixirc_paths(_), do: ["lib"]

  defp deps do
    [
      # Phoenix Channels transport (no Ecto, no HTML, no LiveView)
      {:phoenix, "~> 1.7"},
      {:phoenix_pubsub, "~> 2.1"},
      # WebSocket adapter
      {:bandit, "~> 1.0"},
      # YAML reader for tickets and config
      {:yaml_elixir, "~> 2.9"},
      # YAML writer for field-preserving ticket writes
      {:ymlr, "~> 5.0"},
      # JSON for manifests, run reports, and voice events
      {:jason, "~> 1.4"}
    ]
  end
end
