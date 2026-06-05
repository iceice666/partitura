import Config

# The api_token and WIP limits are loaded from ~/.score/config.yaml
# by Harmony.Config at runtime — secrets are not placed here.
if config_env() == :prod do
  config :harmony, HarmonyWeb.Endpoint,
    http: [port: String.to_integer(System.get_env("PORT") || "4242")]
end
