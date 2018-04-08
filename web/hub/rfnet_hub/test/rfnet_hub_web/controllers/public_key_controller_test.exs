defmodule RfnetHubWeb.PublicKeyControllerTest do
  use RfnetHubWeb.ConnCase

  alias RfnetHub.Account
  alias RfnetHub.Account.PublicKey

  @create_attrs %{key: "some key", last_used: ~D[2010-04-17], name: "some name"}
  @update_attrs %{key: "some updated key", last_used: ~D[2011-05-18], name: "some updated name"}
  @invalid_attrs %{key: nil, last_used: nil, name: nil}

  def fixture(:public_key) do
    {:ok, public_key} = Account.create_public_key(@create_attrs)
    public_key
  end

  setup %{conn: conn} do
    {:ok, conn: put_req_header(conn, "accept", "application/json")}
  end

  describe "index" do
    test "lists all public_keys", %{conn: conn} do
      conn = get conn, public_key_path(conn, :index)
      assert json_response(conn, 200)["data"] == []
    end
  end

  describe "create public_key" do
    test "renders public_key when data is valid", %{conn: conn} do
      conn = post conn, public_key_path(conn, :create), public_key: @create_attrs
      assert %{"id" => id} = json_response(conn, 201)["data"]

      conn = get conn, public_key_path(conn, :show, id)
      assert json_response(conn, 200)["data"] == %{
        "id" => id,
        "key" => "some key",
        "last_used" => ~D[2010-04-17],
        "name" => "some name"}
    end

    test "renders errors when data is invalid", %{conn: conn} do
      conn = post conn, public_key_path(conn, :create), public_key: @invalid_attrs
      assert json_response(conn, 422)["errors"] != %{}
    end
  end

  describe "update public_key" do
    setup [:create_public_key]

    test "renders public_key when data is valid", %{conn: conn, public_key: %PublicKey{id: id} = public_key} do
      conn = put conn, public_key_path(conn, :update, public_key), public_key: @update_attrs
      assert %{"id" => ^id} = json_response(conn, 200)["data"]

      conn = get conn, public_key_path(conn, :show, id)
      assert json_response(conn, 200)["data"] == %{
        "id" => id,
        "key" => "some updated key",
        "last_used" => ~D[2011-05-18],
        "name" => "some updated name"}
    end

    test "renders errors when data is invalid", %{conn: conn, public_key: public_key} do
      conn = put conn, public_key_path(conn, :update, public_key), public_key: @invalid_attrs
      assert json_response(conn, 422)["errors"] != %{}
    end
  end

  describe "delete public_key" do
    setup [:create_public_key]

    test "deletes chosen public_key", %{conn: conn, public_key: public_key} do
      conn = delete conn, public_key_path(conn, :delete, public_key)
      assert response(conn, 204)
      assert_error_sent 404, fn ->
        get conn, public_key_path(conn, :show, public_key)
      end
    end
  end

  defp create_public_key(_) do
    public_key = fixture(:public_key)
    {:ok, public_key: public_key}
  end
end
