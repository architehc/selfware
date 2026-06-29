### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
type Client struct {
    apiKey string
    secret string
    lang   string
    hc     httpDoer
}
=======
type Client struct {
    apiKey string
    secret string
    lang   string
    hc     httpDoer
    // Client type is now unexported
}
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
type Agent interface {
    GetArtistMBID(ctx context.Context, id string, name string) (string, error)
    GetArtistURL(ctx context.Context, id, name, mbid string) (string, error)
    GetArtistBiography(ctx context.Context, id, name, mbid string) (string, error)
    GetSimilarArtists(ctx context.Context, id, name, mbid string, limit int) ([]Artist, error)
    GetArtistImages(ctx context.Context, id, name, mbid string) ([]ExternalImage, error)
    GetArtistTopSongs(ctx context.Context, id, artistName, mbid string, count int) ([]Song, error)
    GetAlbumInfo(ctx context.Context, name, artist, mbid string) (*AlbumInfo, error)
}
=======
type Agent interface {
    GetArtistMBID(ctx context.Context, id string, name string) (string, error)
    GetArtistURL(ctx context.Context, id, name, mbid string) (string, error)
    GetArtistBiography(ctx context.Context, id, name, mbid string) (string, error)
    GetSimilarArtists(ctx context.Context, id, name, mbid string, limit int) ([]Artist, error)
    GetArtistImages(ctx context.Context, id, name, mbid string) ([]ExternalImage, error)
    GetArtistTopSongs(ctx context.Context, id, artistName, mbid string, count int) ([]Song, error)
    GetAlbumInfo(ctx context.Context, name, artist, mbid string) (*AlbumInfo, error)
    // Agent interface is now unexported
}
>>>>>>> REPLACE

### FILE: core/agents/lastfm/lastfm.go
<<<<<<< SEARCH
type Client struct {
    apiKey string
    secret string
    lang   string
    hc     httpDoer
}
=======
type Client struct {
    apiKey string
    secret string
    lang   string
    hc     httpDoer
    // Client type is now unexported
}
>>>>>>> REPLACE

### FILE: core/agents/lastfm/lastfm_agent.go
<<<<<<< SEARCH
type Agent interface {
    GetArtistMBID(ctx context.Context, id string, name string) (string, error)
    GetArtistURL(ctx context.Context, id, name, mbid string) (string, error)
    GetArtistBiography(ctx context.Context, id, name, mbid string) (string, error)
    GetSimilarArtists(ctx context.Context, id, name, mbid string, limit int) ([]Artist, error)
    GetArtistImages(ctx context.Context, id, name, mbid string) ([]ExternalImage, error)
    GetArtistTopSongs(ctx context.Context, id, artistName, mbid string, count int) ([]Song, error)
    GetAlbumInfo(ctx context.Context, name, artist, mbid string) (*AlbumInfo, error)
}
=======
type Agent interface {
    GetArtistMBID(ctx context.Context, id string, name string) (string, error)
    GetArtistURL(ctx context.Context, id, name, mbid string) (string, error)
    GetArtistBiography(ctx context.Context, id, name, mbid string) (string, error)
    GetSimilarArtists(ctx context.Context, id, name, mbid string, limit int) ([]Artist, error)
    GetArtistImages(ctx context.Context, id, name, mbid string) ([]ExternalImage, error)
    GetArtistTopSongs(ctx context.Context, id, artistName, mbid string, count int) ([]Song, error)
    GetAlbumInfo(ctx context.Context, name, artist, mbid string) (*AlbumInfo, error)
    // Agent interface is now unexported
}
>>>>>>> REPLACE

### FILE: core/agents/lastfm/lastfm_client.go
<<<<<<< SEARCH
type Client struct {
    apiKey string
    secret string
    lang   string
    hc     httpDoer
}
=======
type Client struct {
    apiKey string
    secret string
    lang   string
    hc     httpDoer
    // Client type is now unexported
}
>>>>>>> REPLACE