
### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func NewClient(apiKey string, secret string, lang string, hc httpDoer) *Client {
	return &Client{apiKey, secret, lang, hc}
}

type Client struct {
	apiKey string
	secret string
	lang   string
	hc     httpDoer
}

func (c *Client) AlbumGetInfo(ctx context.Context, name string, artist string, mbid string) (*Album, error) {
	params := url.Values{}
	params.Add("method", "album.getInfo")
	params.Add("album", name)
	params.Add("artist", artist)
	params.Add("mbid", mbid)
	params.Add("lang", c.lang)
	response, err := c.makeRequest(ctx, http.MethodGet, params, false)
	if err != nil {
		return nil, err
	}
	return &response.Album, nil
}

func (c *Client) ArtistGetInfo(ctx context.Context, name string, mbid string) (*Artist, error) {
	params := url.Values{}
	params.Add("method", "artist.getInfo")
	params.Add("artist", name)
	params.Add("mbid", mbid)
	params.Add("lang", c.lang)
	response, err := c.makeRequest(ctx, http.MethodGet, params, false)
	if err != nil {
		return nil, err
	}
	return &response.Artist, nil
}

func (c *Client) ArtistGetSimilar(ctx context.Context, name string, mbid string, limit int) (*SimilarArtists, error) {
	params := url.Values{}
	params.Add("method", "artist.getSimilar")
	params.Add("artist", name)
	params.Add("mbid", mbid)
	params.Add("limit", strconv.Itoa(limit))
	response, err := c.makeRequest(ctx, http.MethodGet, params, false)
	if err != nil {
		return nil, err
	}
	return &response.SimilarArtists, nil
}

func (c *Client) ArtistGetTopTracks(ctx context.Context, name string, mbid string, limit int) (*TopTracks, error) {
	params := url.Values{}
	params.Add("method", "artist.getTopTracks")
	params.Add("artist", name)
	params.Add("mbid", mbid)
	params.Add("limit", strconv.Itoa(limit))
	response, err := c.makeRequest(ctx, http.MethodGet, params, false)
	if err != nil {
		return nil, err
	}
	return &response.TopTracks, nil
}

func (c *Client) GetToken(ctx context.Context) (string, error) {
	params := url.Values{}
	params.Add("method", "auth.getToken")
	c.sign(params)
	response, err := c.makeRequest(ctx, http.MethodGet, params, true)
	if err != nil {
		return "", err
	}
	return response.Token, nil
}

func (c *Client) GetSession(ctx context.Context, token string) (string, error) {
	params := url.Values{}
	params.Add("method", "auth.getSession")
	params.Add("token", token)
	response, err := c.makeRequest(ctx, http.MethodGet, params, true)
	if err != nil {
		return "", err
	}
	return response.Session.Key, nil
}
=======
func newClient(apiKey string, secret string, lang string, hc httpDoer) *client {
	return &client{apiKey, secret, lang, hc}
}

type client struct {
	apiKey string
	secret string
	lang   string
	hc     httpDoer
}

func (c *client) albumGetInfo(ctx context.Context, name string, artist string, mbid string) (*Album, error) {
	params := url.Values{}
	params.Add("method", "album.getInfo")
	params.Add("album", name)
	params.Add("artist", artist)
	params.Add("mbid", mbid)
	params.Add("lang", c.lang)
	response, err := c.makeRequest(ctx, http.MethodGet, params, false)
	if err != nil {
		return nil, err
	}
	return &response.Album, nil
}

func (c *client) artistGetInfo(ctx context.Context, name string, mbid string) (*Artist, error) {
	params := url.Values{}
	params.Add("method", "artist.getInfo")
	params.Add("artist", name)
	params.Add("mbid", mbid)
	params.Add("lang", c.lang)
	response, err := c.makeRequest(ctx, http.MethodGet, params, false)
	if err != nil {
		return nil, err
	}
	return &response.Artist, nil
}

func (c *client) artistGetSimilar(ctx context.Context, name string, mbid string, limit int) (*SimilarArtists, error) {
	params := url.Values{}
	params.Add("method", "artist.getSimilar")
	params.Add("artist", name)
	params.Add("mbid", mbid)
	params.Add("limit", strconv.Itoa(limit))
	response, err := c.makeRequest(ctx, http.MethodGet, params, false)
	if err != nil {
		return nil, err
	}
	return &response.SimilarArtists, nil
}

func (c *client) artistGetTopTracks(ctx context.Context, name string, mbid string, limit int) (*TopTracks, error) {
	params := url.Values{}
	params.Add("method", "artist.getTopTracks")
	params.Add("artist", name)
	params.Add("mbid", mbid)
	params.Add("limit", strconv.Itoa(limit))
	response, err := c.makeRequest(ctx, http.MethodGet, params, false)
	if err != nil {
		return nil, err
	}
	return &response.TopTracks, nil
}

func (c *client) getToken(ctx context.Context) (string, error) {
	params := url.Values{}
	params.Add("method", "auth.getToken")
	c.sign(params)
	response, err := c.makeRequest(ctx, http.MethodGet, params, true)
	if err != nil {
		return "", err
	}
	return response.Token, nil
}

func (c *client) getSession(ctx context.Context, token string) (string, error) {
	params := url.Values{}
	params.Add("method", "auth.getSession")
	params.Add("token", token)
	response, err := c.makeRequest(ctx, http.MethodGet, params, true)
	if err != nil {
		return "", err
	}
	return response.Session.Key, nil
}
>>>>>>> REPLACE

### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func (c *Client) UpdateNowPlaying(ctx context.Context, sessionKey string, info ScrobbleInfo) error {
	params := url.Values{}
	params.Add("method", "track.updateNowPlaying")
	params.Add("artist", info.artist)
	params.Add("track", info.track)
	params.Add("album", info.album)
	params.Add("trackNumber", strconv.Itoa(info.trackNumber))
	params.Add("mbid", info.mbid)
	params.Add("duration", strconv.Itoa(info.duration))
	params.Add("albumArtist", info.albumArtist)
	params.Add("sk", sessionKey)
	resp, err := c.makeRequest(ctx, http.MethodPost, params, true)
	if err != nil {
		return err
	}
	if resp.NowPlaying.IgnoredMessage.Code != "0" {
		log.Warn(ctx, "LastFM: NowPlaying was ignored", "code", resp.NowPlaying.IgnoredMessage.Code,
			"text", resp.NowPlaying.IgnoredMessage.Text)
	}
	return nil
}

func (c *Client) Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error {
	params := url.Values{}
	params.Add("method", "track.scrobble")
	params.Add("timestamp", strconv.FormatInt(info.timestamp.Unix(), 10))
	params.Add("artist", info.artist)
	params.Add("track", info.track)
	params.Add("album", info.album)
	params.Add("trackNumber", strconv.Itoa(info.trackNumber))
	params.Add("mbid", info.mbid)
	params.Add("duration", strconv.Itoa(info.duration))
	params.Add("albumArtist", info.albumArtist)
	params.Add("sk", sessionKey)
	resp, err := c.makeRequest(ctx, http.MethodPost, params, true)
	if err != nil {
		return err
	}
	if resp.Scrobbles.Scrobble.IgnoredMessage.Code != "0" {
		log.Warn(ctx, "LastFM: Scrobble was ignored", "code", resp.Scrobbles.Scrobble.IgnoredMessage.Code,
			"text", resp.Scrobbles.Scrobble.IgnoredMessage.Text, "info", info)
	}
	if resp.Scrobbles.Attr.Accepted != 1 {
		log.Warn(ctx, "LastFM: Scrobble was not accepted", "code", resp.Scrobbles.Scrobble.IgnoredMessage.Code,
			"text", resp.Scrobbles.Scrobble.IgnoredMessage.Text, "info", info)
	}
	return nil
}
=======
func (c *client) updateNowPlaying(ctx context.Context, sessionKey string, info ScrobbleInfo) error {
	params := url.Values{}
	params.Add("method", "track.updateNowPlaying")
	params.Add("artist", info.artist)
	params.Add("track", info.track)
	params.Add("album", info.album)
	params.Add("trackNumber", strconv.Itoa(info.trackNumber))
	params.Add("mbid", info.mbid)
	params.Add("duration", strconv.Itoa(info.duration))
	params.Add("albumArtist", info.albumArtist)
	params.Add("sk", sessionKey)
	resp, err := c.makeRequest(ctx, http.MethodPost, params, true)
	if err != nil {
		return err
	}
	if resp.NowPlaying.IgnoredMessage.Code != "0" {
		log.Warn(ctx, "LastFM: NowPlaying was ignored", "code", resp.NowPlaying.IgnoredMessage.Code,
			"text", resp.NowPlaying.IgnoredMessage.Text)
	}
	return nil
}

func (c *client) scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error {
	params := url.Values{}
	params.Add("method", "track.scrobble")
	params.Add("timestamp", strconv.FormatInt(info.timestamp.Unix(), 10))
	params.Add("artist", info.artist)
	params.Add("track", info.track)
	params.Add("album", info.album)
	params.Add("trackNumber", strconv.Itoa(info.trackNumber))
	params.Add("mbid", info.mbid)
	params.Add("duration", strconv.Itoa(info.duration))
	params.Add("albumArtist", info.albumArtist)
	params.Add("sk", sessionKey)
	resp, err := c.makeRequest(ctx, http.MethodPost, params, true)
	if err != nil {
		return err
	}
	if resp.Scrobbles.Scrobble.IgnoredMessage.Code != "0" {
		log.Warn(ctx, "LastFM: Scrobble was ignored", "code", resp.Scrobbles.Scrobble.IgnoredMessage.Code,
			"text", resp.Scrobbles.Scrobble.IgnoredMessage.Text, "info", info)
	}
	if resp.Scrobbles.Attr.Accepted != 1 {
		log.Warn(ctx, "LastFM: Scrobble was not accepted", "code", resp.Scrobbles.Scrobble.IgnoredMessage.Code,
			"text", resp.Scrobbles.Scrobble.IgnoredMessage.Text, "info", info)
	}
	return nil
}
>>>>>>> REPLACE
