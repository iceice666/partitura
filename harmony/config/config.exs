import Config

config :harmony, HarmonyWeb.Endpoint,
  url: [host: "localhost"],
  http: [port: 4242],
  server: false,
  pubsub_server: Harmony.PubSub

config :harmony,
  hook_socket_path: Path.expand("~/.score/harmony.sock"),
  global_config_path: Path.expand("~/.score/config.yaml")

config :logger, :console,
  format: "$time $metadata[$level] $message\n",
  metadata: [:request_id]

import_config "#{config_env()}.exs"
