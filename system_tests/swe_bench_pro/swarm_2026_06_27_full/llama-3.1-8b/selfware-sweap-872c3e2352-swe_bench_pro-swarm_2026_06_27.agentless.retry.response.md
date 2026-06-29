### FILE: core/agents/lastfm/agent.go
<<<<<<< 
func (l *lastfmAgent) NowPlaying(ctx context.Context, userId string, track *model.MediaFile) error {
	sk, err := l.sessionKeys.Get(ctx, userId)
	if err!= nil || sk == "" {
		return scrobbler.ErrNotAuthorized
	}

	err = l.client.UpdateNowPlaying(ctx, sk, ScrobbleInfo{
		artist:      track.Artist,
		track:       track.Title,
		album:       track.Album,
		trackNumber: track.TrackNumber,
		mbid:        track.MbzTrackID,
		duration:    int(track.Duration),
		albumArtist: track.AlbumArtist,
	})
	if err!= nil {
		log.Warn(ctx, "Last.fm client.updateNowPlaying returned error", "track", track.Title, err)
		return scrobbler.ErrUnrecoverable
	}
	return nil
}
>>>>>>> 
func (l *lastfmAgent) NowPlaying(ctx context.Context, userId string, track *model.MediaFile) error {
	sk, err := l.sessionKeys.Get(ctx, userId)
	if err!= nil || sk == "" {
		return scrobbler.ErrNotAuthorized
	}

	err = l.client.UpdateNowPlaying(ctx, sk, ScrobbleInfo{
		artist:      track.Artist,
		track:       track.Title,
		album:       track.Album,
		trackNumber: track.TrackNumber,
		mbid:        track.MbzTrackID,
		duration:    int(track.Duration),
		albumArtist: track.AlbumArtist,
	})
	if err!= nil {
		log.Warn(ctx, "Last.fm client.updateNowPlaying returned error", "track", track.Title, err)
		return scrobbler.ErrUnrecoverable
	}
	return nil
}

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