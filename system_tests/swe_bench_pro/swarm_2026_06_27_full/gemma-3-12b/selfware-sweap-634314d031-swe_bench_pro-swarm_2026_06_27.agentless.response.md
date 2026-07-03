### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func NewClient(apiKey string, secret string, lang string, hc httpDoer) *Client {
	return &Client{apiKey, secret, lang, hc}
}
=======
func NewClient(apiKey string, secret string, lang string, hc httpDoer) *Client {
	return &Client{apiKey: apiKey, secret: secret, lang: lang, hc: hc}
}
>>>>>>> REPLACE

### FILE: core/agents/agents.go
<<<<<<< SEARCH
var _ Interface = (*Agents)(nil)
var _ ArtistMBIDRetriever = (*Agents)(nil)
var _ ArtistURLRetriever = (*Agents)(nil)
var _ ArtistBiographyRetriever = (*Agents)(nil)
var _ ArtistSimilarRetriever = (*Agents)(nil)
var _ ArtistImageRetriever = (*Agents)(nil)
var _ ArtistTopSongsRetriever = (*Agents)(nil)
var _ AlbumInfoRetriever = (*Agents)(nil)
=======
var _ Interface = (*Agents)(nil)
var _ ArtistMBIDRetriever = (*LastFMClient)(nil)
var _ ArtistURLRetriever = (*LastFMClient)(nil)
var _ ArtistBiographyRetriever = (*LastFMClient)(nil)
var _ ArtistSimilarRetriever = (*LastFMClient)(nil)
var _ ArtistImageRetriever = (*LastFMClient)(nil)
var _ ArtistTopSongsRetriever = (*LastFMClient)(nil)
var _ AlbumInfoRetriever = (*LastFMClient)(nil)
>>>>>>> REPLACE
