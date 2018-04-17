defmodule RfnetHubWeb.FallbackController do
  @moduledoc """
  Translates controller action results into valid `Plug.Conn` responses.

  See `Phoenix.Controller.action_fallback/1` for more details.
  """
  use RfnetHubWeb, :controller

  def call(conn, {:error, %Ecto.Changeset{} = changeset}) do
    conn
    |> put_status(:unprocessable_entity)
    |> render(RfnetHubWeb.ChangesetView, "error.json", changeset: changeset)
  end

  def call(conn, {:error, _key, %Ecto.Changeset{} = changeset, %{}}) do
    conn
    |> put_status(:unprocessable_entity)
    |> render(RfnetHubWeb.ChangesetView, "error.json", changeset: changeset)
  end

  def call(conn, {:error, :not_found}) do
    conn
    |> put_status(:not_found)
    |> render(RfnetHubWeb.ErrorView, :"404")
  end

  def call(conn, other) do
    IO.inspect(other)
    conn
  end
end
