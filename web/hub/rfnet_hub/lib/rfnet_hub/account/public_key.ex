defmodule RfnetHub.Account.PublicKey do
  use Ecto.Schema
  import Ecto.Changeset


  schema "public_keys" do
    field :key, :binary
    field :last_used, :date
    field :name, :string
    field :user_id, :id

    timestamps()
  end

  @doc false
  def changeset(public_key, attrs) do
    public_key
    |> cast(attrs, [:key, :name, :last_used, :user_id])
    |> validate_required([:key, :name, :last_used])
  end
end
