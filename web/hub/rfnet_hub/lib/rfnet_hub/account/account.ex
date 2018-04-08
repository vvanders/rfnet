defmodule RfnetHub.Account do
  @moduledoc """
  The Account context.
  """

  import Ecto.Query, warn: false
  alias RfnetHub.Repo

  alias RfnetHub.Account.User

  @doc """
  Returns the list of users.

  ## Examples

      iex> list_users()
      [%User{}, ...]

  """
  def list_users do
    Repo.all(User)
  end

  @doc """
  Gets a single user.

  Raises `Ecto.NoResultsError` if the User does not exist.

  ## Examples

      iex> get_user!(123)
      %User{}

      iex> get_user!(456)
      ** (Ecto.NoResultsError)

  """
  def get_user!(id), do: Repo.get!(User, id)

  @doc """
  Creates a user.

  ## Examples

      iex> create_user(%{field: value})
      {:ok, %User{}}

      iex> create_user(%{field: bad_value})
      {:error, %Ecto.Changeset{}}

  """
  def create_user(attrs \\ %{}) do
    %User{}
    |> User.changeset(attrs)
    |> Repo.insert()
  end

  @doc """
  Updates a user.

  ## Examples

      iex> update_user(user, %{field: new_value})
      {:ok, %User{}}

      iex> update_user(user, %{field: bad_value})
      {:error, %Ecto.Changeset{}}

  """
  def update_user(%User{} = user, attrs) do
    user
    |> User.changeset(attrs)
    |> Repo.update()
  end

  @doc """
  Deletes a User.

  ## Examples

      iex> delete_user(user)
      {:ok, %User{}}

      iex> delete_user(user)
      {:error, %Ecto.Changeset{}}

  """
  def delete_user(%User{} = user) do
    Repo.delete(user)
  end

  @doc """
  Returns an `%Ecto.Changeset{}` for tracking user changes.

  ## Examples

      iex> change_user(user)
      %Ecto.Changeset{source: %User{}}

  """
  def change_user(%User{} = user) do
    User.changeset(user, %{})
  end

  alias RfnetHub.Account.PublicKey

  @doc """
  Returns the list of public_keys.

  ## Examples

      iex> list_public_keys()
      [%PublicKey{}, ...]

  """
  def list_public_keys do
    Repo.all(PublicKey)
  end

  @doc """
  Gets a single public_key.

  Raises `Ecto.NoResultsError` if the Public key does not exist.

  ## Examples

      iex> get_public_key!(123)
      %PublicKey{}

      iex> get_public_key!(456)
      ** (Ecto.NoResultsError)

  """
  def get_public_key!(id), do: Repo.get!(PublicKey, id)

  @doc """
  Creates a public_key.

  ## Examples

      iex> create_public_key(%{field: value})
      {:ok, %PublicKey{}}

      iex> create_public_key(%{field: bad_value})
      {:error, %Ecto.Changeset{}}

  """
  def create_public_key(attrs \\ %{}) do
    %PublicKey{}
    |> PublicKey.changeset(attrs)
    |> Repo.insert()
  end

  @doc """
  Updates a public_key.

  ## Examples

      iex> update_public_key(public_key, %{field: new_value})
      {:ok, %PublicKey{}}

      iex> update_public_key(public_key, %{field: bad_value})
      {:error, %Ecto.Changeset{}}

  """
  def update_public_key(%PublicKey{} = public_key, attrs) do
    public_key
    |> PublicKey.changeset(attrs)
    |> Repo.update()
  end

  @doc """
  Deletes a PublicKey.

  ## Examples

      iex> delete_public_key(public_key)
      {:ok, %PublicKey{}}

      iex> delete_public_key(public_key)
      {:error, %Ecto.Changeset{}}

  """
  def delete_public_key(%PublicKey{} = public_key) do
    Repo.delete(public_key)
  end

  @doc """
  Returns an `%Ecto.Changeset{}` for tracking public_key changes.

  ## Examples

      iex> change_public_key(public_key)
      %Ecto.Changeset{source: %PublicKey{}}

  """
  def change_public_key(%PublicKey{} = public_key) do
    PublicKey.changeset(public_key, %{})
  end
end
