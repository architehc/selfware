### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func (c *Client) GetArtistURL(ctx context.Context, id, name, mbid string) (string, error) {
	params := url.Values{}
	params.Add("method", "artist.getInfo")
	params.Add("artist", name)
	params.Add("mbid", mbid)
	params.Add("lang", c.lang)
	response, err := c.makeRequest(ctx, http.MethodGet, params, false)
	if err != nil {
		return "", err
	}
	return response.Artist.URL, nil
}
=======
func (c *Client) GetArtistURL(ctx context.Context, id, name, mbid string) (string, error) {
	return "", fmt.Errorf("Invalid Method - No method with that name in this package")
}
>>>>>>> REPLACE