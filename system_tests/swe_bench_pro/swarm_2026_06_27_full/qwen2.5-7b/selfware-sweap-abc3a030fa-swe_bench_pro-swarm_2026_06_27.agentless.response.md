### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func NewClient(apiKey string, secret string, lang string, hc httpDoer) *Client {
	return &Client{apiKey, secret, lang, hc}
}
=======
func NewClient(apiKey string, secret string, lang string, hc httpDoer) *Client {
	return &Client{apiKey, secret, lang, hc}
}
privateClient *Client
=======
privateClient *Client
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
func lastFMConstructor(ds model.DataStore) *lastfmAgent {
	l := &lastfmAgent{
		ds:          ds,
		lang:        conf.Server.LastFM.Language,
		apiKey:      conf.Server.LastFM.ApiKey,
		secret:      conf.Server.LastFM.Secret,
		sessionKeys: &agents.SessionKeys{DataStore: ds, KeyName: sessionKeyProperty},
	}
	hc := &http.Client{
		Timeout: consts.DefaultHttpClientTimeOut,
	}
	chc := utils.NewCachedHTTPClient(hc, consts.DefaultHttpClientTimeOut)
	l.client = NewClient(l.apiKey, l.secret, l.lang, chc)
	return l
}
=======
func lastFMConstructor(ds model.DataStore) *lastfmAgent {
	l := &lastfmAgent{
		ds:          ds,
		lang:        conf.Server.LastFM.Language,
		apiKey:      conf.Server.LastFM.ApiKey,
		secret:      conf.Server.LastFM.Secret,
		sessionKeys: &agents.SessionKeys{DataStore: ds, KeyName: sessionKeyProperty},
	}
	hc := &http.Client{
		Timeout: consts.DefaultHttpClientTimeOut,
	}
	chc := utils.NewCachedHTTPClient(hc, consts.DefaultHttpClientTimeOut)
	privateClient = NewClient(l.apiKey, l.secret, l.lang, chc)
	l.client = privateClient
	return l
}
>>>>>>> REPLACE