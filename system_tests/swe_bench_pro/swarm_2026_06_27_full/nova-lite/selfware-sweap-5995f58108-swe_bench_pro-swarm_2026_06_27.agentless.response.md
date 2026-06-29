### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
type Client struct {
	apiKey string
	secret string
	lang   string
	hc     httpDoer
}
=======
type client struct {
	apiKey string
	secret string
	lang   string
	hc     httpDoer
}
>>>>>>> REPLACE

### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func (c *Client) AlbumGetInfo(ctx context.Context, name string, artist string, mbid string) (*Album, error) {
=======
func (c *client) AlbumGetInfo(ctx context.Context, name string, artist string, mbid string) (*Album, error) {
>>>>>>> REPLACE

### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func (c *Client) ArtistGetInfo(ctx context.Context, name string, mbid string) (*Artist, error) {
=======
func (c *client) ArtistGetInfo(ctx context.Context, name string, mbid string) (*Artist, error) {
>>>>>>> REPLACE

### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func (c *Client) ArtistGetSimilar(ctx context.Context, name string, mbid string, limit int) (*SimilarArtists, error) {
=======
func (c *client) ArtistGetSimilar(ctx context.Context, name string, mbid string, limit int) (*SimilarArtists, error) {
>>>>>>> REPLACE

### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func (c *Client) ArtistGetTopTracks(ctx context.Context, name string, mbid string, limit int) (*TopTracks, error) {
=======
func (c *client) ArtistGetTopTracks(ctx context.Context, name string, mbid string, limit int) (*TopTracks, error) {
>>>>>>> REPLACE

### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func (c *Client) GetToken(ctx context.Context) (string, error) {
=======
func (c *client) GetToken(ctx context.Context) (string, error) {
>>>>>>> REPLACE

### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func (c *Client) GetSession(ctx context.Context, token string) (string, error) {
=======
func (c *client) GetSession(ctx context.Context, token string) (string, error) {
>>>>>>> REPLACE

### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func (c *Client) UpdateNowPlaying(ctx context.Context, sessionKey string, info ScrobbleInfo) error {
=======
func (c *client) UpdateNowPlaying(ctx context.Context, sessionKey string, info ScrobbleInfo) error {
>>>>>>> REPLACE

### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func (c *Client) Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error {
=======
func (c *client) Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error {
>>>>>>> REPLACE

### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func (c *Client) makeRequest(ctx context.Context, method string, params url.Values, signed bool) (*Response, error) {
=======
func (c *client) makeRequest(ctx context.Context, method string, params url.Values, signed bool) (*Response, error) {
>>>>>>> REPLACE

### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func (c *Client) sign(params url.Values) {
=======
func (c *client) sign(params url.Values) {
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
	l.client = NewClient(l.apiKey, l.secret, l.lang, chc)
=======
	l.client = NewClient(l.apiKey, l.secret, l.lang, chc)
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
	a, err := l.client.AlbumGetInfo(ctx, name, artist, mbid)
=======
	a, err := l.client.AlbumGetInfo(ctx, name, artist, mbid)
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
	a, err := l.client.ArtistGetInfo(ctx, name, mbid)
=======
	a, err := l.client.ArtistGetInfo(ctx, name, mbid)
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
	s, err := l.client.ArtistGetSimilar(ctx, name, mbid, limit)
=======
	s, err := l.client.ArtistGetSimilar(ctx, name, mbid, limit)
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
	t, err := l.client.ArtistGetTopTracks(ctx, artistName, mbid, count)
=======
	t, err := l.client.ArtistGetTopTracks(ctx, artistName, mbid, count)
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
	err = l.client.UpdateNowPlaying(ctx, sk, ScrobbleInfo{
=======
	err = l.client.UpdateNowPlaying(ctx, sk, ScrobbleInfo{
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
	err = l.client.Scrobble(ctx, sk, ScrobbleInfo{
=======
	err = l.client.Scrobble(ctx, sk, ScrobbleInfo{
>>>>>>> REPLACE