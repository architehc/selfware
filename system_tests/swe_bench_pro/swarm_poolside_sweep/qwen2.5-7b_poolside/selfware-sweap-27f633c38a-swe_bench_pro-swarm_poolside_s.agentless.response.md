### FILE: core/agents/lastfm/client.go
<<<<<<< SEARCH
func NewClient(apiKey string, secret string, lang string, hc httpDoer) *Client {
=======
func NewClient(apiKey string, secret string, lang string, hc httpDoer) *Client {
    client := &Client{apiKey, secret, lang, hc}
    return client
}
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
func (l *lastfmAgent) GetAlbumInfo(ctx context.Context, name, artist, mbid string) (*agents.AlbumInfo, error) {
=======
func (l *lastfmAgent) GetAlbumInfo(ctx context.Context, name, artist, mbid string) (*agents.AlbumInfo, error) {
    album, err := l.client.AlbumGetInfo(ctx, name, artist, mbid)
    if err != nil {
        return nil, err
    }
    response := agents.AlbumInfo{
        Name:        album.Name,
        MBID:        album.MBID,
        Description: album.Description.Summary,
        URL:         album.URL,
        Images:      make([]agents.ExternalImage, 0),
    }

    // Last.fm can return duplicate sizes.
    seenSizes := map[int]bool{}

    // This assumes that Last.fm returns images with size small, medium, and large.
    // This is true as of December 29, 2022
    for _, img := range album.Image {
        size := imageRegex.FindStringSubmatch(img.URL)
        // Last.fm can return images without URL
        if len(size) == 0 || len(size[0]) < 4 {
            log.Trace(ctx, "LastFM/albuminfo image URL does not match expected regex or is empty", "url", img.URL, "size", img.Size)
            continue
        }

        numericSize, err := strconv.Atoi(size[0][2:])
        if err != nil {
            log.Error(ctx, "LastFM/albuminfo image URL does not match expected regex", "url", img.URL, "size", img.Size, err)
            return nil, err
        } else {
            if _, exists := seenSizes[numericSize]; !exists {
                response.Images = append(response.Images, agents.ExternalImage{
                    Size: numericSize,
                    URL:  img.URL,
                })
                seenSizes[numericSize] = true
            }
        }
    }

    return &response, nil
}
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
func (l *lastfmAgent) GetArtistMBID(ctx context.Context, id string, name string) (string, error) {
=======
func (l *lastfmAgent) GetArtistMBID(ctx context.Context, id string, name string) (string, error) {
    artist, err := l.client.ArtistGetInfo(ctx, name, "")
    if err != nil {
        return "", err
    }
    if artist.MBID == "" {
        return "", agents.ErrNotFound
    }
    return artist.MBID, nil
}
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
func (l *lastfmAgent) GetArtistURL(ctx context.Context, id, name, mbid string) (string, error) {
=======
func (l *lastfmAgent) GetArtistURL(ctx context.Context, id, name, mbid string) (string, error) {
    artist, err := l.client.ArtistGetInfo(ctx, name, mbid)
    if err != nil {
        return "", err
    }
    if artist.URL == "" {
        return "", agents.ErrNotFound
    }
    return artist.URL, nil
}
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
func (l *lastfmAgent) GetArtistBiography(ctx context.Context, id, name, mbid string) (string, error) {
=======
func (l *lastfmAgent) GetArtistBiography(ctx context.Context, id, name, mbid string) (string, error) {
    artist, err := l.client.ArtistGetInfo(ctx, name, mbid)
    if err != nil {
        return "", err
    }
    if artist.Bio.Summary == "" {
        return "", agents.ErrNotFound
    }
    return artist.Bio.Summary, nil
}
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
func (l *lastfmAgent) GetSimilarArtists(ctx context.Context, id, name, mbid string, limit int) ([]agents.Artist, error) {
=======
func (l *lastfmAgent) GetSimilarArtists(ctx context.Context, id, name, mbid string, limit int) ([]agents.Artist, error) {
    artists, err := l.client.ArtistGetSimilar(ctx, name, mbid, limit)
    if err != nil {
        return nil, err
    }
    var res []agents.Artist
    for _, a := range artists {
        res = append(res, agents.Artist{
            Name: a.Name,
            MBID: a.MBID,
        })
    }
    return res, nil
}
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
func (l *lastfmAgent) GetArtistTopSongs(ctx context.Context, id, artistName, mbid string, count int) ([]agents.Song, error) {
=======
func (l *lastfmAgent) GetArtistTopSongs(ctx context.Context, id, artistName, mbid string, count int) ([]agents.Song, error) {
    tracks, err := l.client.ArtistGetTopTracks(ctx, artistName, mbid, count)
    if err != nil {
        return nil, err
    }
    var res []agents.Song
    for _, t := range tracks {
        res = append(res, agents.Song{
            Name: t.Name,
            MBID: t.MBID,
        })
    }
    return res, nil
}
>>>>>>> REPLACE