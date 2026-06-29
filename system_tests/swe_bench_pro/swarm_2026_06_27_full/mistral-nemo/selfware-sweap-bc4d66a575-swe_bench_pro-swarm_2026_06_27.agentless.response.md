### FILE: lib/kube/proxy/forwarder.go
<<<<<<< SEARCH
		// Wrap stdin/out/err with data trackers, but keep them as nil if they were
		// nil. Otherwise, executor will try to use these tracking read/writers
		// when the underlying stream is nil.
		trackIn := utils.NewTrackingReader(streamOptions.Stdin)
		if streamOptions.Stdin != nil {
			streamOptions.Stdin = trackIn
		}
		trackOut := utils.NewTrackingWriter(streamOptions.Stdout)
		if streamOptions.Stdout != nil {
			streamOptions.Stdout = trackOut
		}
		trackErr := utils.NewTrackingWriter(streamOptions.Stderr)
		if streamOptions.Stderr != nil {
			streamOptions.Stderr = trackErr
		}
		if recorder != nil {
			// capture stderr and stdout writes to session recorder
			streamOptions.Stdout = utils.NewBroadcastWriter(streamOptions.Stdout, recorder)
			streamOptions.Stderr = utils.NewBroadcastWriter(streamOptions.Stderr, recorder)
		}

		if err = executor.Stream(streamOptions); err != nil {
			f.log.WithError(err).Warning("Executor failed while streaming.")
			return nil, trace.Wrap(err)
		}
		if err := proxy.sendStatus(err); err != nil {
			f.log.WithError(err).Warning("Failed to send status. Exec command was aborted by client.")
			return nil, trace.Wrap(err)
		}
=======
		// Wrap stdin/out/err with data trackers, but keep them as nil if they were
		// nil. Otherwise, executor will try to use these tracking read/writers
		// when the underlying stream is nil.
		trackIn := utils.NewTrackingReader(streamOptions.Stdin)
		if streamOptions.Stdin != nil {
			streamOptions.Stdin = trackIn
		}
		trackOut := utils.NewTrackingWriter(streamOptions.Stdout)
		if streamOptions.Stdout != nil {
			streamOptions.Stdout = trackOut
		}
		trackErr := utils.NewTrackingWriter(streamOptions.Stderr)
		if streamOptions.Stderr != nil {
			streamOptions.Stderr = trackErr
		}
		if recorder != nil {
			// capture stderr and stdout writes to session recorder
			streamOptions.Stdout = utils.NewBroadcastWriter(streamOptions.Stdout, recorder)
			streamOptions.Stderr = utils.NewBroadcastWriter(streamOptions.Stderr, recorder)
		}

		if err = executor.Stream(streamOptions); err != nil {
			f.log.WithError(err).Warning("Executor failed while streaming.")
			return nil, trace.Wrap(err)
		}
		if err := proxy.sendStatus(err); err != nil {
			f.log.WithError(err).Warning("Failed to send status. Exec command was aborted by client.")
			return nil, trace.Wrap(err)
		}

		// Create the upload directory if it doesn't exist
		if err := os.MkdirAll("/var/lib/teleport/log/upload/streaming/default", 0755); err != nil {
			f.log.WithError(err).Warning("Failed to create upload directory.")
			return nil, trace.Wrap(err)
		}
>>>>>>> REPLACE