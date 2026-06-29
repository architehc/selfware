diff --git a/core/agents/lastfm/client.go b/core/agents/lastfm/client.go
index 7a4f4a3f..e5a4d3f3 100644
--- a/core/agents/lastfm/client.go
+++ b/core/agents/lastfm/client.go
@@ -1,6 +1,6 @@
 package lastfm

-import (
-	"context"
-	"crypto/md5"
-	"encoding/hex"
-	"encoding/json"
-	"fmt"
-	"net/http"
-	"net/url"
-	"sort"
+import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"sort"

@@ -10,6 +9,7 @@
 	"github.com/navidrome/navidrome/log"
 	"github.com/navidrome/navidrome/utils"
 )

+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -17,7 +18,7 @@
 	// NewClient returns a new LastFM client.
 	NewClient(apiKey, secret, lang, client *http.Client) *Client
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -25,7 +26,7 @@
 	// makeRequest makes a request to the LastFM API.
 	makeRequest(ctx context.Context, method string, params url.Values, signed bool) (*Response, error)
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -33,7 +34,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -41,7 +42,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -50,7 +51,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -58,7 +59,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -66,7 +67,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -74,7 +75,7 @@
 	// ArtistMBIDRetriever is an interface for retrieving an artist's MBID.
 	ArtistMBIDRetriever interface {
 		GetArtistMBID(ctx context.Context, id string, name string) (string, error)
 	}

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -82,7 +83,7 @@
 	// NewClient returns a new LastFM client.
 	NewClient(apiKey, secret, lang, client *http.Client) *Client
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -90,7 +91,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -98,7 +99,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -107,7 +108,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -115,7 +116,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -123,7 +124,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -131,7 +132,7 @@
 	// makeRequest makes a request to the LastFM API.
 	makeRequest(ctx context.Context, method string, params url.Values, signed bool) (*Response, error)
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -139,7 +140,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -147,7 +148,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -156,7 +157,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -164,7 +165,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -172,7 +173,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -180,7 +181,7 @@
 	// NewClient returns a new LastFM client.
 	NewClient(apiKey, secret, lang, client *http.Client) *Client
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -188,7 +189,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -196,7 +197,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -205,7 +206,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -213,7 +214,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -221,7 +222,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -229,7 +230,7 @@
 	// makeRequest makes a request to the LastFM API.
 	makeRequest(ctx context.Context, method string, params url.Values, signed bool) (*Response, error)
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -237,7 +238,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -245,7 +246,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -254,7 +255,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -262,7 +263,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -270,7 +271,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -278,7 +279,7 @@
 	// NewClient returns a new LastFM client.
 	NewClient(apiKey, secret, lang, client *http.Client) *Client
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -286,7 +287,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -294,7 +295,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -303,7 +304,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -311,7 +312,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -319,7 +320,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -327,7 +328,7 @@
 	// makeRequest makes a request to the LastFM API.
 	makeRequest(ctx context.Context, method string, params url.Values, signed bool) (*Response, error)
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -335,7 +336,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -343,7 +344,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -352,7 +353,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -360,7 +361,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -368,7 +369,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -376,7 +377,7 @@
 	// NewClient returns a new LastFM client.
 	NewClient(apiKey, secret, lang, client *http.Client) *Client
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -384,7 +385,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -392,7 +393,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -401,7 +402,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -409,7 +410,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -417,7 +418,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -425,7 +426,7 @@
 	// makeRequest makes a request to the LastFM API.
 	makeRequest(ctx context.Context, method string, params url.Values, signed bool) (*Response, error)
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -433,7 +434,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -441,7 +442,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -450,7 +451,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -458,7 +459,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -466,7 +467,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -474,7 +475,7 @@
 	// NewClient returns a new LastFM client.
 	NewClient(apiKey, secret, lang, client *http.Client) *Client
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -482,7 +483,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -490,7 +491,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -499,7 +500,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -507,7 +508,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -515,7 +516,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -523,7 +524,7 @@
 	// makeRequest makes a request to the LastFM API.
 	makeRequest(ctx context.Context, method string, params url.Values, signed bool) (*Response, error)
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -531,7 +532,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -539,7 +540,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -548,7 +549,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -556,7 +557,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -564,7 +565,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -572,7 +573,7 @@
 	// NewClient returns a new LastFM client.
 	NewClient(apiKey, secret, lang, client *http.Client) *Client
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -580,7 +581,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -588,7 +589,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -597,7 +598,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -605,7 +606,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -613,7 +614,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -621,7 +622,7 @@
 	// makeRequest makes a request to the LastFM API.
 	makeRequest(ctx context.Context, method string, params url.Values, signed bool) (*Response, error)
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -629,7 +630,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -637,7 +638,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -646,7 +647,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -654,7 +655,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -662,7 +663,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -670,7 +671,7 @@
 	// NewClient returns a new LastFM client.
 	NewClient(apiKey, secret, lang, client *http.Client) *Client
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -678,7 +679,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -686,7 +687,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -695,7 +696,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -703,7 +704,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json:"scrobble"`
 }

@@ -711,7 +712,7 @@
 	// Scrobble represents a scrobble.
 	Scrobble struct {
 		Accepted int `json:"accepted"`
 	} `json:"scrobble"`
 }

-type Client struct {
+type client struct {
 	*http.Client
 	apiKey string
 	secret string
 	lang   string
@@ -719,7 +720,7 @@
 	// makeRequest makes a request to the LastFM API.
 	makeRequest(ctx context.Context, method string, params url.Values, signed bool) (*Response, error)
 }

-type Response struct {
+type response struct {
 	Artist         Artist         `json:"artist"`
 	SimilarArtists SimilarArtists `json:"similarartists"`
 	TopTracks      TopTracks      `json:"toptracks"`
@@ -727,7 +728,7 @@
 	// Scrobble sends a scrobble to the LastFM API.
 	Scrobble(ctx context.Context, sessionKey string, info ScrobbleInfo) error
 }

-type ScrobbleInfo struct {
+type scrobbleInfo struct {
 	Timestamp time.Time `json:"timestamp"`
 	Artist    string    `json:"artist"`
 	Track     string    `json:"track"`
@@ -735,7 +736,7 @@
 	// Artist represents an artist.
 	Artist struct {
 		Name string `json:"name"`
 	} `json:"artist"`
 }

-type Artist struct {
+type artist struct {
 	Name string `json:"name"`
 }

-type SimilarArtists struct {
+type similarArtists struct {
 	Artists []Artist `json:"artist"`
 }

@@ -744,7 +745,7 @@
 	// TopTracks represents a list of top tracks.
 	TopTracks struct {
 		Tracks []Track `json:"track"`
 	} `json:"toptracks"`
 }

-type Track struct {
+type track struct {
 	Name string `json:"name"`
 	Mbid  string `json:"mbid"`
 	Duration int `json:"duration"`
@@ -752,7 +753,7 @@
 	// Album represents an album.
 	Album struct {
 		Name string `json:"name"`
 	} `json:"album"`
 }

-type Session struct {
+type session struct {
 	Token string `json:"token"`
 }

-type Scrobbles struct {
+type scrobbles struct {
 	Scrobble Scrobble `json