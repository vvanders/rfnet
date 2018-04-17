defmodule RfnetHubWeb.UserController do
  use RfnetHubWeb, :controller

  alias RfnetHub.Account
  alias RfnetHub.Account.User
  alias RfnetHub.Account.PublicKey

  plug RfnetHubWeb.SignedPlug, %{domain: "rfnet.net", auth_host: "http://sign"}

  action_fallback RfnetHubWeb.FallbackController

  def index(conn, _params) do
    users = Account.list_users()
    IO.inspect("foo")
    render(conn, "index.json", users: users)
  end

  def create(conn, %{"user" => user_params, "public_key" => public_key}) do
    IO.inspect(user_params, label: "create")

    with {:ok, %{ :user => %User{} = user, :key => %PublicKey{} = _key }} <- Account.create_user(user_params, public_key) do
      conn
      |> put_status(:created)
      |> put_resp_header("location", user_path(conn, :show, user))
      |> render("show.json", user: user)
    end
  end

  def show(conn, %{"id" => id}) do
    user = Account.get_user!(id)
    render(conn, "show.json", user: user)
  end

  def update(conn, %{"id" => id, "user" => user_params}) do
    user = Account.get_user!(id)

    with {:ok, %User{} = user} <- Account.update_user(user, user_params) do
      render(conn, "show.json", user: user)
    end
  end

  def delete(conn, %{"id" => id}) do
    user = Account.get_user!(id)
    with {:ok, %User{}} <- Account.delete_user(user) do
      send_resp(conn, :no_content, "")
    end
  end
end