# This file is responsible for configuring your application
# and its dependencies with the aid of the Mix.Config module.
#
# This configuration file is loaded before any dependency and
# is restricted to this project.
use Mix.Config

# General application configuration
config :rfnet_hub,
  ecto_repos: [RfnetHub.Repo]

# Configures the endpoint
config :rfnet_hub, RfnetHubWeb.Endpoint,
  url: [host: "localhost"],
  secret_key_base: "GzgYpSyNoN5MBeYKM/ojdxuImAmChALPAEFhQl8aUk7cb6UPcuKoxhVrwiuVvrmP",
  render_errors: [view: RfnetHubWeb.ErrorView, accepts: ~w(json)],
  pubsub: [name: RfnetHub.PubSub,
           adapter: Phoenix.PubSub.PG2]

# Configures Elixir's Logger
config :logger, :console,
  format: "$time $metadata[$level] $message\n",
  metadata: [:request_id]

# Import environment specific config. This must remain at the bottom
# of this file so it overrides the configuration defined above.
import_config "#{Mix.env}.exs"
