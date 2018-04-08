defmodule RfnetHubWeb.UserView do
  use RfnetHubWeb, :view
  alias RfnetHubWeb.UserView

  def render("index.json", %{users: users}) do
    %{data: render_many(users, UserView, "user.json")}
  end

  def render("show.json", %{user: user}) do
    %{data: render_one(user, UserView, "user.json")}
  end

  def render("user.json", %{user: user}) do
    %{id: user.id,
      callsign: user.callsign,
      email: user.email,
      password: user.password,
      has_password: user.has_password,
      verified: user.verified}
  end
end
