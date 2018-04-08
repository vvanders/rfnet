defmodule RfnetHub.AccountTest do
  use RfnetHub.DataCase

  alias RfnetHub.Account

  describe "users" do
    alias RfnetHub.Account.User

    @valid_attrs %{callsign: "some callsign", email: "some email", has_password: true, password: "some password", verified: true}
    @update_attrs %{callsign: "some updated callsign", email: "some updated email", has_password: false, password: "some updated password", verified: false}
    @invalid_attrs %{callsign: nil, email: nil, has_password: nil, password: nil, verified: nil}

    def user_fixture(attrs \\ %{}) do
      {:ok, user} =
        attrs
        |> Enum.into(@valid_attrs)
        |> Account.create_user()

      user
    end

    test "list_users/0 returns all users" do
      user = user_fixture()
      assert Account.list_users() == [user]
    end

    test "get_user!/1 returns the user with given id" do
      user = user_fixture()
      assert Account.get_user!(user.id) == user
    end

    test "create_user/1 with valid data creates a user" do
      assert {:ok, %User{} = user} = Account.create_user(@valid_attrs)
      assert user.callsign == "some callsign"
      assert user.email == "some email"
      assert user.has_password == true
      assert user.password == "some password"
      assert user.verified == true
    end

    test "create_user/1 with invalid data returns error changeset" do
      assert {:error, %Ecto.Changeset{}} = Account.create_user(@invalid_attrs)
    end

    test "update_user/2 with valid data updates the user" do
      user = user_fixture()
      assert {:ok, user} = Account.update_user(user, @update_attrs)
      assert %User{} = user
      assert user.callsign == "some updated callsign"
      assert user.email == "some updated email"
      assert user.has_password == false
      assert user.password == "some updated password"
      assert user.verified == false
    end

    test "update_user/2 with invalid data returns error changeset" do
      user = user_fixture()
      assert {:error, %Ecto.Changeset{}} = Account.update_user(user, @invalid_attrs)
      assert user == Account.get_user!(user.id)
    end

    test "delete_user/1 deletes the user" do
      user = user_fixture()
      assert {:ok, %User{}} = Account.delete_user(user)
      assert_raise Ecto.NoResultsError, fn -> Account.get_user!(user.id) end
    end

    test "change_user/1 returns a user changeset" do
      user = user_fixture()
      assert %Ecto.Changeset{} = Account.change_user(user)
    end
  end

  describe "public_keys" do
    alias RfnetHub.Account.PublicKey

    @valid_attrs %{key: "some key", last_used: ~D[2010-04-17], name: "some name"}
    @update_attrs %{key: "some updated key", last_used: ~D[2011-05-18], name: "some updated name"}
    @invalid_attrs %{key: nil, last_used: nil, name: nil}

    def public_key_fixture(attrs \\ %{}) do
      {:ok, public_key} =
        attrs
        |> Enum.into(@valid_attrs)
        |> Account.create_public_key()

      public_key
    end

    test "list_public_keys/0 returns all public_keys" do
      public_key = public_key_fixture()
      assert Account.list_public_keys() == [public_key]
    end

    test "get_public_key!/1 returns the public_key with given id" do
      public_key = public_key_fixture()
      assert Account.get_public_key!(public_key.id) == public_key
    end

    test "create_public_key/1 with valid data creates a public_key" do
      assert {:ok, %PublicKey{} = public_key} = Account.create_public_key(@valid_attrs)
      assert public_key.key == "some key"
      assert public_key.last_used == ~D[2010-04-17]
      assert public_key.name == "some name"
    end

    test "create_public_key/1 with invalid data returns error changeset" do
      assert {:error, %Ecto.Changeset{}} = Account.create_public_key(@invalid_attrs)
    end

    test "update_public_key/2 with valid data updates the public_key" do
      public_key = public_key_fixture()
      assert {:ok, public_key} = Account.update_public_key(public_key, @update_attrs)
      assert %PublicKey{} = public_key
      assert public_key.key == "some updated key"
      assert public_key.last_used == ~D[2011-05-18]
      assert public_key.name == "some updated name"
    end

    test "update_public_key/2 with invalid data returns error changeset" do
      public_key = public_key_fixture()
      assert {:error, %Ecto.Changeset{}} = Account.update_public_key(public_key, @invalid_attrs)
      assert public_key == Account.get_public_key!(public_key.id)
    end

    test "delete_public_key/1 deletes the public_key" do
      public_key = public_key_fixture()
      assert {:ok, %PublicKey{}} = Account.delete_public_key(public_key)
      assert_raise Ecto.NoResultsError, fn -> Account.get_public_key!(public_key.id) end
    end

    test "change_public_key/1 returns a public_key changeset" do
      public_key = public_key_fixture()
      assert %Ecto.Changeset{} = Account.change_public_key(public_key)
    end
  end
end
