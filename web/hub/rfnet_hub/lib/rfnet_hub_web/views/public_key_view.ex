defmodule RfnetHubWeb.PublicKeyView do
  use RfnetHubWeb, :view
  alias RfnetHubWeb.PublicKeyView

  def render("index.json", %{public_keys: public_keys}) do
    %{data: render_many(public_keys, PublicKeyView, "public_key.json")}
  end

  def render("show.json", %{public_key: public_key}) do
    %{data: render_one(public_key, PublicKeyView, "public_key.json")}
  end

  def render("public_key.json", %{public_key: public_key}) do
    %{id: public_key.id,
      key: public_key.key,
      name: public_key.name,
      last_used: public_key.last_used}
  end
end
