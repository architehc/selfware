### FILE: core/agents/lastfm/agent.go
<<<<<<< 
func (l *lastfmAgent) GetArtistInfo(ctx context.Context, name string, mbid string) (*Artist, error) {
	a, err := l.client.ArtistGetInfo(ctx, name, mbid)
	var lfErr *lastFMError
	isLastFMError := errors.As(err, &lfErr)

	if mbid!= "" && ((err == nil && a.Name == "[unknown]") || (isLastFMError && lfErr.Code == 6)) {
		log.Warn(ctx, "LastFM/artist.getInfo could not find artist by mbid, trying again", "artist", name, "mbid", mbid)
		return l.GetArtistInfo(ctx, name, "")
	}

	if err!= nil {
		log.Error(ctx, "Error calling LastFM/artist.getInfo", "artist", name, "mbid", mbid, err)
		return nil, err
	}
	return a, nil
}
>>>>>>> 
func (l *lastfmAgent) GetArtistInfo(ctx context.Context, name string, mbid string) (*Artist, error) {
	a, err := l.client.ArtistGetInfo(ctx, name, mbid)
	var lfErr *lastFMError
	isLastFMError := errors.As(err, &lfErr)

	if mbid!= "" && ((err == nil && a.Name == "[unknown]") || (isLastFMError && lfErr.Code == 6)) {
		log.Warn(ctx, "LastFM/artist.getInfo could not find artist by mbid, trying again", "artist", name, "mbid", mbid)
		return l.GetArtistInfo(ctx, name, "")
	}

	if err!= nil {
		log.Error(ctx, "Error calling LastFM/artist.getInfo", "artist", name, "mbid", mbid, err)
		return nil, err
	}
	return a, nil
}

### FILE: core/agents/lastfm/responses.go
<<<<<<< 
type Artist struct {
	Name  string          `json:"name"`
	MBID  string          `json:"mbid"`
	URL   string          `json:"url"`
	Image []ExternalImage `json:"image"`
	Bio   Description     `json:"bio"`
}
>>>>>>> 
type Artist struct {
	Name  string          `json:"name"`
	MBID  string          `json:"mbid"`
	URL   string          `json:"url"`
	Image []ExternalImage `json:"image"`
	Bio   Description     `json:"bio"`
}

### FILE: core/agents/agents.go
<<<<<<< 
func (a *Agents) GetArtistInfo(ctx context.Context, id, name, mbid string) (*agents.AlbumInfo, error) {
	start := time.Now()
	for _, ag := range a.agents {
		if utils.IsCtxDone(ctx) {
			break
		}
		agent, ok := ag.(ArtistInfoRetriever)
		if!ok {
			continue
		}
		a, err := agent.GetArtistInfo(ctx, id, name, mbid)
		if err == nil {
			log.Debug(ctx, "Got Artist Info", "agent", ag.AgentName(), "artist", name, "elapsed", time.Since(start))
			return a, nil
		}
	}
	return nil, ErrNotFound
}
>>>>>>> 
func (a *Agents) GetArtistInfo(ctx context.Context, id, name, mbid string) (*agents.AlbumInfo, error) {
	start := time.Now()
	for _, ag := range a.agents {
		if utils.IsCtxDone(ctx) {
			break
		}
		agent, ok := ag.(ArtistInfoRetriever)
		if!ok {
			continue
		}
		a, err := agent.GetArtistInfo(ctx, id, name, mbid)
		if err == nil {
			log.Debug(ctx, "Got Artist Info", "agent", ag.AgentName(), "artist", name, "elapsed", time.Since(start))
			return a, nil
		}
	}
	return nil, ErrNotFound
}

### FILE: core/agents/agents.go
<<<<<<< 
var _ Interface = (*Agents)(nil)
var _ ArtistMBIDRetriever = (*Agents)(nil)
var _ ArtistURLRetriever = (*Agents)(nil)
var _ ArtistBiographyRetriever = (*Agents)(nil)
var _ ArtistSimilarRetriever = (*Agents)(nil)
var _ ArtistImageRetriever = (*Agents)(nil)
var _ ArtistTopSongsRetriever = (*Agents)(nil)
var _ AlbumInfoRetriever = (*Agents)(nil)
>>>>>>> 
var _ Interface = (*Agents)(nil)
var _ ArtistInfoRetriever = (*Agents)(nil)
var _ ArtistMBIDRetriever = (*Agents)(nil)
var _ ArtistURLRetriever = (*Agents)(nil)
var _ ArtistBiographyRetriever = (*Agents)(nil)
var _ ArtistSimilarRetriever = (*Agents)(nil)
var _ ArtistImageRetriever = (*Agents)(nil)
var _ ArtistTopSongsRetriever = (*Agents)(nil)
var _ AlbumInfoRetriever = (*Agents)(nil)