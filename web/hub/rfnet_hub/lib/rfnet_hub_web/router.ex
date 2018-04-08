defmodule RfnetHubWeb.Router do
  use RfnetHubWeb, :router

  pipeline :api do
    plug :accepts, ["json"]
  end

  scope "/api/v1", RfnetHubWeb do
    pipe_through :api

    resources "/users", UserController, except: [:new, :edit]
    resources "/public_keys", PublicKeyController, except: [:new, :edit]
  end
end
