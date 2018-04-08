defmodule RfnetHubWeb.PublicKeyController do
  use RfnetHubWeb, :controller

  alias RfnetHub.Account
  alias RfnetHub.Account.PublicKey

  action_fallback RfnetHubWeb.FallbackController

  def index(conn, _params) do
    public_keys = Account.list_public_keys()
    render(conn, "index.json", public_keys: public_keys)
  end

  def create(conn, %{"public_key" => public_key_params}) do
    with {:ok, %PublicKey{} = public_key} <- Account.create_public_key(public_key_params) do
      conn
      |> put_status(:created)
      |> put_resp_header("location", public_key_path(conn, :show, public_key))
      |> render("show.json", public_key: public_key)
    end
  end

  def show(conn, %{"id" => id}) do
    public_key = Account.get_public_key!(id)
    render(conn, "show.json", public_key: public_key)
  end

  def update(conn, %{"id" => id, "public_key" => public_key_params}) do
    public_key = Account.get_public_key!(id)

    with {:ok, %PublicKey{} = public_key} <- Account.update_public_key(public_key, public_key_params) do
      render(conn, "show.json", public_key: public_key)
    end
  end

  def delete(conn, %{"id" => id}) do
    public_key = Account.get_public_key!(id)
    with {:ok, %PublicKey{}} <- Account.delete_public_key(public_key) do
      send_resp(conn, :no_content, "")
    end
  end
end
