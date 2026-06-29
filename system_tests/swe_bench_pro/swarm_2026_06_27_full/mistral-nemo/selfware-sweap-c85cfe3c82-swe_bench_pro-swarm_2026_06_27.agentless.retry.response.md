### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
type Client struct {
	apiKey string
	secret string
	lang   string
	hc     httpDoer
}
=======
type lastFMClient struct {
	apiKey string
	secret string
	lang   string
	hc     httpDoer
}

func NewLastFMClient(apiKey string, secret string, lang string, hc httpDoer) *lastFMClient {
	return &lastFMClient{apiKey, secret, lang, hc}
}
>>>>>>> REPLACE