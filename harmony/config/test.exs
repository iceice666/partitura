import Config

config :harmony, HarmonyWeb.Endpoint,
  http: [port: 4001],
  server: false

config :harmony,
  hook_socket_path: "/tmp/harmony_test.sock",
  start_hook_receiver: false

config :logger, level: :warning
