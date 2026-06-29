### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
// newStreamer returns sync or async streamer based on the configuration
// of the server and the session, sync streamer sends the events
// directly to the auth server and blocks if the events can not be received,
// async streamer buffers the events to disk and uploads the events later
func (f *Forwarder) newStreamer(ctx *authContext) (events.Streamer, error) {
	mode := ctx.clusterConfig.GetSessionRecording()
	if services.IsRecordSync(mode) {
		f.log.Debugf("Using sync streamer for session.")
		return f.Client, nil
	}
	f.log.Debugf("Using async streamer for session.")
	dir := filepath.Join(
		f.DataDir, teleport.LogsDir, teleport.ComponentUpload,
		events.StreamingLogsDir, defaults.Namespace,
	)
	fileStreamer, err := filesessions.NewStreamer(dir)
	if err != nil {
		return nil, trace.Wrap(err)
	}
	// TeeStreamer sends non-print and non disk events
	// to the audit log in async mode, while buffering all
	// events on disk for further upload at the end of the session
	return events.NewTeeStreamer(fileStreamer, f.StreamEmitter), nil
}
=======
// newStreamer returns sync or async streamer based on the configuration
// of the server and the session, sync streamer sends the events
// directly to the auth server and blocks if the events can not be received,
// async streamer buffers the events to disk and uploads the events later
func (f *Forwarder) newStreamer(ctx *authContext) (events.Streamer, error) {
	mode := ctx.clusterConfig.GetSessionRecording()
	if services.IsRecordSync(mode) {
		f.log.Debugf("Using sync streamer for session.")
		return f.Client, nil
	}
	f.log.Debugf("Using async streamer for session.")
	dir := filepath.Join(
		f.DataDir, teleport.LogsDir, teleport.ComponentUpload,
		events.StreamingLogsDir, defaults.Namespace,
	)
	// Ensure the directory exists before creating the streamer.
	if err := os.MkdirAll(dir, 0755); err != nil {
		return nil, trace.Wrap(err)
	}
	fileStreamer, err := filesessions.NewStreamer(dir)
	if err != nil {
		return nil, trace.Wrap(err)
	}
	// TeeStreamer sends non-print and non disk events
	// to the audit log in async mode, while buffering all
	// events on disk for further upload at the end of the session
	return events.NewTeeStreamer(fileStreamer, f.StreamEmitter), nil
}
>>>>>>> REPLACE

### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
import (
	"crypto/tls"
	"fmt"
	"io"
	"net"
	"net/http"
	"path/filepath"
	"strings"
	"time"

	"github.com/gravitational/teleport"
	"github.com/gravitational/teleport/lib/auth"
	"github.com/gravitational/teleport/lib/defaults"
	"github.com/gravitational/teleport/lib/events"
	"github.com/gravitational/teleport/lib/events/filesessions"
	"github.com/gravitational/teleport/lib/httplib"
	"github.com/gravitational/teleport/lib/kube/utils"
	"github.com/gravitational/teleport/lib/reversetunnel"
	"github.com/gravitational/teleport/lib/services"
	"github.com/gravitational/teleport/lib/session"
	"github.com/gravitational/teleport/lib/srv"
	"github.com/gravitational/teleport/lib/utils"
	"github.com/gravitational/trace"
	log "github.com/sirupsen/logrus"
	"github.com/julienschmidt/httprouter"
	"k8s.io/apimachinery/pkg/util/httpstream"
	"k8s.io/apimachinery/pkg/util/httpstream/spdy"
	"k8s.io/client-go/tools/remotecommand"
	utilexec "k8s.io/client-go/util/exec"
)
=======
import (
	"crypto/tls"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/gravitational/teleport"
	"github.com/gravitational/teleport/lib/auth"
	"github.com/gravitational/teleport/lib/defaults"
	"github.com/gravitational/teleport/lib/events"
	"github.com/gravitational/teleport/lib/events/filesessions"
	"github.com/gravitational/teleport/lib/httplib"
	"github.com/gravitational/teleport/lib/kube/utils"
	"github.com/gravitational/teleport/lib/reversetunnel"
	"github.com/gravitational/teleport/lib/services"
	"github.com/gravitational/teleport/lib/session"
	"github.com/gravitational/teleport/lib/srv"
	"github.com/gravitational/teleport/lib/utils"
	"github.com/gravitational/trace"
	log "github.com/sirupsen/logrus"
	"github.com/julienschmidt/httprouter"
	"k8s.io/apimachinery/pkg/util/httpstream"
	"k8s.io/apimachinery/pkg/util/httpstream/spdy"
	"k8s.io/client-go/tools/remotecommand"
	utilexec "k8s.io/client-go/util/exec"
)
>>>>>>> REPLACE