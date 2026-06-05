defmodule HarmonyWeb.UserSocket do
  @moduledoc """
  WebSocket authentication — local shared-secret token.

  Clients must pass ?token=<api_token> on connect. The api_token is loaded
  from ~/.score/config.yaml by Harmony.Config at startup.

  """
  use Phoenix.Socket

  channel("projects:lobby", HarmonyWeb.LobbyChannel)
  channel("project:*", HarmonyWeb.ProjectChannel)

  @impl true
  def connect(%{"token" => token}, socket, _connect_info) do
    case Harmony.Config.api_token() do
      expected when is_binary(expected) and expected == token -> {:ok, socket}
      _ -> :error
    end
  end

  def connect(_params, _socket, _info), do: :error

  @impl true
  def id(_socket), do: nil
end
