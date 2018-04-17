defmodule RfnetHubWeb.SignedPlug do
    import Plug.Conn

    def init(default), do: default

    def call(conn, %{:domain => domain, :auth_host => auth_host}) do
        with [signature] <- get_req_header(conn, "x-rfnet-signature"),
             [sequence_id] <- get_req_header(conn, "x-rfnet-sequence_id"),
             [sender] <- get_req_header(conn, "x-rfnet-sender"),
             body <- conn.assigns[:body_copy] do

            IO.inspect("Checking sig")

            {sequence_id, _} = Integer.parse(sequence_id)
            auth_request = Poison.encode!(%{
                sequence_id: sequence_id,
                addr: sender,
                signature: signature,
                url: conn.request_path,
                headers: [],
                method: conn.method,
                body: Base.encode64(body),
                public_keys: [
                    "MTIzNDU2Nzg5MDEyMzQ1Njc4OTAxMjM0NTY3ODkwMjM="
                ]
            })

            sign_result = HTTPotion.post auth_host, [body: auth_request, headers: ["Content-Type": "application/json"]]

            with %HTTPotion.Response { status_code: 200, body: body } <- sign_result,
                %{ "verified" => true } <- Poison.decode!(body) do
                    conn |> assign(:sig_verified, true)
            else
                e -> conn
                    |> resp(400, "Signature check failed")
                    |> halt
            end
        else
            _ -> conn
        end
    end
end

# Custom parser so we can copy body out when it's parsed
defmodule Plug.Parsers.JSON_WITH_BODY do
  @behaviour Plug.Parsers
  import Plug.Conn

  def init(opts) do
    {decoder, opts} = Keyword.pop(opts, :json_decoder)

    unless decoder do
      raise ArgumentError, "JSON parser expects a :json_decoder option"
    end

    {decoder, opts}
  end

  def parse(conn, "application", subtype, _headers, {decoder, opts}) do
    if subtype == "json" or String.ends_with?(subtype, "+json") do
      conn
      |> read_body(opts)
      |> decode(decoder)
    else
      {:next, conn}
    end
  end

  def parse(conn, _type, _subtype, _headers, _opts) do
    {:next, conn}
  end

  defp decode({:ok, "", conn}, _decoder) do
    {:ok, %{}, conn |> assign(:body_copy, "")}
  end

  defp decode({:ok, body, conn}, decoder) do
    IO.inspect("decode json")

    case apply_mfa_or_module(body, decoder) do
      terms when is_map(terms) ->
        {:ok, terms, conn |> assign(:body_copy, body)}

      terms ->
        {:ok, %{"_json" => terms}, conn |> assign(:body_copy, body)}
    end
  rescue
    e -> raise Plug.Parsers.ParseError, exception: e
  end

  defp decode({:more, _, conn}, _decoder) do
    {:error, :too_large, conn}
  end

  defp decode({:error, :timeout}, _decoder) do
    raise Plug.TimeoutError
  end

  defp decode({:error, _}, _decoder) do
    raise Plug.BadRequestError
  end

  defp apply_mfa_or_module(body, {module_name, function_name, extra_args}) do
    apply(module_name, function_name, [body | extra_args])
  end

  defp apply_mfa_or_module(body, decoder) do
    decoder.decode!(body)
  end
end