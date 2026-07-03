diff --git a/src/routes/index.js b/src/routes/index.js
index 3a5b3f3..e4a4f2c 100644
--- a/src/routes/index.js
+++ b/src/routes/index.js
@@ -1,5 +1,6 @@
 const router = require('./middleware');
 const userRoutes = require('./user');
+const wellKnownRoutes = require('./well-known');
 
 router.get('/api/me', require('./middleware').ensureLoggedIn(), userRoutes.getMe);
 router.get('/api/me/edit', require('./middleware').ensureLoggedIn(), userRoutes.getEdit);
@@ -14,6 +15,7 @@
 router.get('/api/me/edit/password', require('./middleware').ensureLoggedIn(), userRoutes.getEditPassword);
 router.get('/api/me/edit/email', require('./middleware').ensureLoggedIn(), userRoutes.getEditEmail);
 router.get('/api/me/edit/username', require('./middleware').ensureLoggedIn(), userRoutes.getEditUsername);
+router.get('/api/.well-known/webfinger', wellKnownRoutes.getWebfinger);
 
 module.exports = router;

diff --git a/src/controllers/well-known.js b/src/controllers/well-known.js
index 0000000..e4a4f2c
--- /dev/null
+++ b/src/controllers/well-known.js
@@ -0,0 +1,34 @@
+const nconf = require('nconf');
+const { ensureLoggedIn, ensureGuest } = require('./middleware');
+const { getUidFromUsername } = require('./user');
+
+module.exports = {
+  getWebfinger: async (req, res) => {
+    const resource = req.query.resource;
+    if (!resource ||!resource.startsWith('acct:') ||!resource.endsWith(nconf.get('url_parsed').hostname)) {
+      return res.status(400).send({ error: 'Invalid resource' });
+    }
+
+    const uid = getUidFromUsername(resource.slice(6));
+    if (!uid) {
+      return res.status(404).send({ error: 'User not found' });
+    }
+
+    if (!req.uid &&!ensureGuest(req, 'groups:view:users')) {
+      return res.status(403).send({ error: 'Forbidden' });
+    }
+
+    const aliases = [
+      `https://${nconf.get('url_parsed').hostname}/user/${uid}`,
+      `https://${nconf.get('url_parsed').hostname}/user/${uid}/profile`,
+    ];
+
+    const links = [
+      {
+        rel: 'alternate',
+        type: 'text/html',
+        href: `https://${nconf.get('url_parsed').hostname}/user/${uid}`,
+      },
+    ];
+
+    res.json({
+      subject: resource,
+      aliases,
+      links,
+    });
+  },
+};

diff --git a/src/routes/well-known.js b/src/routes/well-known.js
index 0000000..e4a4f2c
--- /dev/null
+++ b/src/routes/well-known.js
@@ -0,0 +1,6 @@
+const router = require('express').Router();
+const wellKnownController = require('../controllers/well-known');
+
+router.get('/change-password', (req, res) => {
+  res.redirect('/me/edit/password');
+});
+
+module.exports = router;

diff --git a/src/prestart.js b/src/prestart.js
index 3a5b3f3..e4a4f2c 100644
--- a/src/prestart.js
+++ b/src/prestart.js
@@ -84,7 +84,7 @@
   84 | 	nconf.set('upload_url', '/assets/uploads');
   85 | 
   86 | 
   87 | 	// nconf defaults, if not set in config
   88 | 	if (!nconf.get('sessionKey')) {
   89 | 		nconf.set('sessionKey', 'express.sid');
   90 | 	}
   91 | 
   92 | 	if (nconf.get('url')) {
   93 | 		nconf.set('url', nconf.get('url').replace(/\/$/, ''));
   94 | 		nconf.set('url_parsed', url.parse(nconf.get('url')));
   95 | 		// Parse out the relative_url and other goodies from the configured URL
   96 | 		const urlObject = url.parse(nconf.get('url'));
   97 | 		const relativePath = urlObject.pathname!== '/'? urlObject.pathname.replace(/\/+$/, '') : '';
   98 | 		nconf.set('base_url', `${urlObject.protocol}//${urlObject.host}`);
   99 | 		nconf.set('secure', urlObject.protocol === 'https:');
  100 | 		nconf.set('use_port',!!urlObject.port);
  101 | 		nconf.set('relative_path', relativePath);
  102 | 		if (!nconf.get('asset_base_url')) {
  103 | 			nconf.set('asset_base_url', `${relativePath}/assets`);
  104 | 		}
  105 | +		nconf.set('well_known_routes', true);
  106 | 	}

diff --git a/test/mocks/databasemock.js b/test/mocks/databasemock.js
index 3a5b3f3..e4a4f2c 100644
--- a/test/mocks/databasemock.js
+++ b/test/mocks/databasemock.js
@@ -44,7 +44,7 @@
   44 | 	views_dir: path.join(__dirname, '../../build/public/templates'),
   45 | 	relative_path: '',
   46 | });
   47 | 
   48 | const urlObject = url.parse(nconf.get('url'));
   49 | const relativePath = urlObject.pathname!== '/'? urlObject.pathname : '';
   50 | nconf.set('relative_path', relativePath);
   51 | nconf.set('asset_base_url', `${relativePath}/assets`);
   52 | nconf.set('upload_path', path.join(nconf.get('base_dir'), nconf.get('upload_path')));
   53 | nconf.set('upload_url', '/assets/uploads');
   54 | nconf.set('url_parsed', urlObject);
   55 | nconf.set('base_url', `${urlObject.protocol}//${urlObject.host}`);
   56 | nconf.set('secure', urlObject.protocol === 'https:');
   57 | nconf.set('use_port',!!urlObject.port);
   58 | nconf.set('port', urlObject.port || nconf.get('port') || (nconf.get('PORT_ENV_VAR')? nconf.get(nconf.get('PORT_ENV_VAR')) : false) || 4567);
   59 | 
   60 | // cookies don't provide isolation by port: http://stackoverflow.com/a/16328399/122353
   61 | const domain = nconf.get('cookieDomain') || urlObject.hostname;
   62 | const origins = nconf.get('socket.io:origins') || `${urlObject.protocol}//${domain}:*`;
   63 | nconf.set('socket.io:origins', origins);
   64 | +	nconf.set('well_known_routes', true);

diff --git a/webpack.installer.js b/webpack.installer.js
index 3a5b3f3..e4a4f2c 100644
--- a/webpack.installer.js
+++ b/webpack.installer.js
@@ -1,5 +1,6 @@
 const path = require('path');
 
 module.exports = {
   mode: 'production',
   entry: {
     installer: './public/src/installer/install.js',
   },
   output: {
     filename: '[name].min.js',
     path: path.resolve(__dirname, 'build/public'),
     publicPath: `/assets/`,
   },
   resolve: {
     symlinks: false,
     modules: [
       'public/src',
       'node_modules',
     ],
   },
   plugins: [
     new webpack.DefinePlugin({
       'process.env.NODE_ENV': JSON.stringify('production'),
     }),
   ],
 };

diff --git a/loader.js b/loader.js
index 3a5b3f3..e4a4f2c 100644
--- a/loader.js
+++ b/loader.js
@@ -1,5 +1,6 @@
 const nconf = require('nconf');
 const fs = require('fs');
 const url = require('url');
 const path = require('path');
 const { fork } = require('child_process');
 const logrotate = require('logrotate-stream');
 const { mkdirp } = require('mkdirp');
 const file = require('./src/file');
 const pkg = require('./package.json');
@@ -14,6 +15,7 @@
 const pidFilePath = path.join(__dirname, 'pidfile');
 
 const outputLogFilePath = path.join(__dirname, nconf.get('logFile') || 'logs/output.log');
 const logDir = path.dirname(outputLogFilePath);
 if (!fs.existsSync(logDir)) {
   mkdirp.sync(path.dirname(outputLogFilePath));
 }
@@ -29,6 +31,7 @@
 const silent = nconf.get('silent') === 'false'? false : nconf.get('silent')!== false;
 let numProcs;
 const workers = [];
 const Loader = {};
 const appPath = path.join(__dirname, 'app.js');
 Loader.init = function () {
   if (silent) {
     console.log = (...args) => {
       output.write(`${args.join(' ')}\n`);
     };
   }
@@ -43,6 +46,7 @@
 process.on('SIGHUP', Loader.restart);
 process.on('SIGTERM', Loader.stop);
 };
 Loader.displayStartupMessages = function () {
   console.log('');
   console.log(`NodeBB v${pkg.version} Copyright (C) 2013-${(new Date()).getFullYear()} NodeBB Inc.`);
   console.log('This program comes with ABSOLUTELY NO WARRANTY.');
@@ -54,6 +58,7 @@
 const { ensureLoggedIn, ensureGuest } = require('./middleware');
 const { getUidFromUsername } = require('./user');
 
 Loader.start = function () {
   const app = require(appPath);
   const server = app.listen(nconf.get('port'), () => {
     console.log(`Listening on port ${nconf.get('port')}`);
@@ -71,6 +76,7 @@
   });
 };
 Loader.restart = function () {
   console.log('Restarting NodeBB...');
   Loader.stop();
   Loader.start();
 };
 Loader.stop = function () {
   console.log('Stopping NodeBB...');
   server.close();
   process.exit(0);
 };