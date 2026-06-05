import Config

config :harmony, HarmonyWeb.Endpoint,
  http: [port: 4242],
  server: true

config :logger, level: :debug
