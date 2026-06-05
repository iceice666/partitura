defmodule HarmonyWeb.Endpoint do
  use Phoenix.Endpoint, otp_app: :harmony

  socket("/socket", HarmonyWeb.UserSocket,
    websocket: true,
    longpoll: false
  )

  plug(Plug.RequestId)
  plug(Plug.Head)
end
