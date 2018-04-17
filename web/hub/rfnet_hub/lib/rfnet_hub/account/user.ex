defmodule RfnetHub.Account.User do
  use Ecto.Schema
  import Ecto.Changeset


  schema "users" do
    field :callsign, :string
    field :email, :string
    field :has_password, :boolean, default: false
    field :password, :string
    field :verified, :boolean, default: false

    timestamps()
  end

  @doc false
  def changeset(user, attrs) do
    user
    |> cast(attrs, [:callsign, :email, :password, :has_password, :verified])
    |> unique_constraint(:callsign)
    |> unique_constraint(:email)
    |> validate_required([:callsign, :email, :password, :has_password, :verified])
  end
end
