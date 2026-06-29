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
    // Make the Client type package-private by removing the exported declaration
}
>>>>>>> REPLACE

### FILE: core/agents/lastfm/agent.go
<<<<<<< SEARCH
type lastfmAgent struct {
    ds          model.DataStore
    sessionKeys *agents.SessionKeys
    apiKey      string
    secret      string
    lang        string
    client      *Client
}
=======
type lastfmAgent struct {
    ds          model.DataStore
    sessionKeys *agents.SessionKeys
    apiKey      string
    secret      string
    lang        string
    client      *Client
    // Make the lastfmAgent type package-private by removing the exported declaration
}
>>>>>>> REPLACE