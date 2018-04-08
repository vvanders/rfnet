defmodule RfnetHub.Repo.Migrations.CreatePublicKeys do
  use Ecto.Migration

  def change do
    create table(:public_keys) do
      add :key, :binary
      add :name, :string
      add :last_used, :date
      add :user_id, references(:users, on_delete: :nothing)

      timestamps()
    end

    create index(:public_keys, [:user_id])
  end
end
