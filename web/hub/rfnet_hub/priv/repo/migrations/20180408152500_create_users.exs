defmodule RfnetHub.Repo.Migrations.CreateUsers do
  use Ecto.Migration

  def change do
    create table(:users) do
      add :callsign, :string
      add :email, :string
      add :password, :string
      add :has_password, :boolean, default: false, null: false
      add :verified, :boolean, default: false, null: false

      timestamps()
    end

  end
end
