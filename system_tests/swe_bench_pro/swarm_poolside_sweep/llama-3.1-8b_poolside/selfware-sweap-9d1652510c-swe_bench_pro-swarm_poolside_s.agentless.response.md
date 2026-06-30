### FILE: core/agents/lastfm/client.go
<<<<<<< 
func NewClient(apiKey string, secret string, lang string, hc httpDoer) *Client {
	return &Client{apiKey, secret, lang, hc}
}
=======
func NewClient(apiKey string, secret string, lang string, hc httpDoer) (*Client, error) {
	return &Client{apiKey, secret, lang, hc}, nil
}
>>>>>>> 

### FILE: core/agents/agents.go
<<<<<<< 
func New(ds model.DataStore) *Agents {
	var order []string
	if conf.Server.Agents!= "" {
		order = strings.Split(conf.Server.Agents, ",")
	}
	order = append(order, LocalAgentName)
	var res []Interface
	for _, name := range order {
		init, ok := Map[name]
		if!ok {
			log.Error("Agent not available. Check configuration", "name", name)
			continue
		}

		res = append(res, init(ds))
	}

	return &Agents{ds: ds, agents: res}
}
=======
func New(ds model.DataStore) (*Agents, error) {
	var order []string
	if conf.Server.Agents!= "" {
		order = strings.Split(conf.Server.Agents, ",")
	}
	order = append(order, LocalAgentName)
	var res []Interface
	for _, name := range order {
		init, ok := Map[name]
		if!ok {
			log.Error("Agent not available. Check configuration", "name", name)
			continue
		}

		res = append(res, init(ds))
	}

	return &Agents{ds: ds, agents: res}, nil
}
>>>>>>> 

### FILE: core/agents/lastfm/client.go
<<<<<<< 
func (c *Client) makeRequest(ctx context.Context, method string, params url.Values, signed bool) (*Response, error) {
	params.Add("format", "json")
	params.Add("api_key", c.apiKey)

	if signed {
		c.sign(params)
	}

	req, _ := http.NewRequestWithContext(ctx, method, apiBaseUrl, nil)
	req.URL.RawQuery = params.Encode()

	resp, err := c.hc.Do(req)
	if err!= nil {
		return nil, err
	}

	defer resp.Body.Close()
	decoder := json.NewDecoder(resp.Body)

	var response Response
	jsonErr := decoder.Decode(&response)
	if resp.StatusCode!= 200 && jsonErr!= nil {
		return nil, fmt.Errorf("last.fm http status: (%d)", resp.StatusCode)
	}
	if jsonErr!= nil {
		return nil, jsonErr
	}
	if response.Error!= 0 {
		return &response, &lastFMError{Code: response.Error, Message: response.Message}
	}

	return &response, nil
}
>>>>>>> 

### FILE: core/agents/lastfm/client.go
<<<<<<< 
func (c *Client) sign(params url.Values) {
	// the parameters must be in order before hashing
	keys := make([]string, 0, len(params))
	for k := range params {
		if slices.Contains([]string{"format", "callback"}, k) {
			continue
		}
		keys = append(keys, k)
	}
	sort.Strings(keys)
	msg := strings.Builder{}
	for _, k := range keys {
		msg.WriteString(k)
		msg.WriteString(params[k][0])
	}
	msg.WriteString(c.secret)
	hash := md5.Sum([]byte(msg.String()))
	params.Add("api_sig", hex.EncodeToString(hash[:]))
}
>>>>>>> 

### FILE: core/agents/lastfm/client.go
<<<<<<< 
type Client struct {
	apiKey string
	secret string
	lang   string
	hc     httpDoer
}
>>>>>>> 

### FILE: core/agents/lastfm/client.go
<<<<<<< 
func (c *Client) AlbumGetInfo(ctx context.Context, name string, artist string, mbid string) (*Album, error) {
	params := url.Values{}
	params.Add("method", "album.getInfo")
	params.Add("album", name)
	params.Add("artist", artist)
	params.Add("mbid", mbid)
	params.Add("lang", c.lang)
	response, err := c.makeRequest(ctx, http.MethodGet, params, false)
	if err!= nil {
		return nil, err
	}
	return &response.Album, nil
}
>>>>>>> 

### FILE: core/agents/lastfm/client.go
<<<<<<< 
func (c *Client) ArtistGetInfo(ctx context.Context, name string, mbid string) (*Artist, error) {
	params := url.Values{}
	params.Add("method", "artist.getInfo")
	params.Add("artist", name)
	params.Add("mbid", mbid)
	params.Add("lang", c.lang)
	response, err := c.makeRequest(ctx, http.MethodGet, params, false)
	if err!= nil {
		return nil, err
	}
	return &response.Artist, nil
}
>>>>>>> 

### FILE: core/agents/lastfm/client.go
<<<<<<< 
func (c *Client) ArtistGetSimilar(ctx context.Context, name string, mbid string, limit int) (*SimilarArtists, error) {
	params := url.Values{}
	params.Add("method", "artist.getSimilar")
	params.Add("artist", name)
	params.Add("mbid", mbid)
	params.Add("limit", strconv.Itoa(limit))
	response, err := c.makeRequest(ctx, http.MethodGet, params, false)
	if err!= nil {
		return nil, err
	}
	return &response.SimilarArtists, nil
}
>>>>>>> 

### FILE: core/agents/lastfm/client.go
<<<<<<< 
func (c *Client) ArtistGetTopTracks(ctx context.Context, name string, mbid string, limit int) (*TopTracks, error) {
	params := url.Values{}
	params.Add("method", "artist.getTopTracks")
	params.Add("artist", name)
	params.Add("mbid", mbid)
	params.Add("limit", strconv.Itoa(limit))
	response, err := c.makeRequest(ctx, http.MethodGet, params, false)
	if err!= nil {
		return nil, err
	}
	return &response.TopTracks, nil
}
>>>>>>> 

### FILE: core/agents/lastfm/client.go
<<<<<<< 
func (c *Client) GetToken(ctx context.Context) (string, error) {
	params := url.Values{}
	params.Add("method", "auth.getToken")
	c.sign(params)
	response, err := c.makeRequest(ctx, http.MethodGet, params, true)
	if err!= nil {
		return "", err
	}
	return response.Token, nil
}
>>>>>>> 

### FILE: core/agents/lastfm/client.go
<<<<<<< 
func (c *Client) GetSession(ctx context.Context, token string) (string, error) {
	params := url.Values{}
	params.Add("method", "auth.getSession")
	params.Add("token", token)
	response, err := c.makeRequest(ctx, http.MethodGet, params, true)
	if err!= nil {
		return "", err
	}
	return response.Session.Key, nil
}
>>>>>>> 

### FILE: core/agents/lastfm/client.go
<<<<<<< 
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
	if err!= nil {
		return err
	}
	if resp.NowPlaying.IgnoredMessage.Code!= "0" {
		log.Warn(ctx, "LastFM: NowPlaying was ignored", "code", resp.NowPlaying.IgnoredMessage.Code,
			"text", resp.NowPlaying.IgnoredMessage.Text)
	}
	return nil
}
>>>>>>> 

### FILE: core/agents/lastfm/client.go
<<<<<<< 
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
	if err!= nil {
		return err
	}
	if resp.Scrobbles.Scrobble.IgnoredMessage.Code!= "0" {
		log.Warn(ctx, "LastFM: Scrobble was ignored", "code", resp.Scrobbles.Scrobble.IgnoredMessage.Code,
			"text", resp.Scrobbles.Scrobble.IgnoredMessage.Text, "info", info)
	}
	if resp.Scrobbles.Attr.Accepted!= 1 {
		log.Warn(ctx, "LastFM: Scrobble was not accepted", "code", resp.Scrobbles.Scrobble.IgnoredMessage.Code,
			"text", resp.Scrobbles.Scrobble.IgnoredMessage.Text, "info", info)
	}
	return nil
}
>>>>>>>