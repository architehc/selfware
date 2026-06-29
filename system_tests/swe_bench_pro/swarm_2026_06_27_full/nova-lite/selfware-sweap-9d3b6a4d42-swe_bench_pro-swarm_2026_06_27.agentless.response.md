### FILE: src/controllers/well-known.js
<<<<<<< SEARCH
=======
"use strict";

module.exports = function (router) {
    router.get("/.well-known/webfinger", async (req, res, next) => {
        const resource = req.query.resource;
        if (!resource ||!resource.startsWith("acct:") ||!resource.endsWith(`${nconf.get('url_parsed').hostname}`)) {
            return res.status(400).end();
        }
        const username = resource.split("@")[0].replace("acct:", "");
        try {
            const user = await user.getUserByField('username', username);
            if (!user) {
                return res.status(404).end();
            }
            if (!await privileges.user.has(req.uid || 'guests', 'groups:view:users')) {
                return res.status(403).end();
            }
            const response = {
                subject: resource,
                aliases: [`${nconf.get('url')}/${user.username}`, `${nconf.get('url')}/${user.slug}`],
                links: [{ rel: 'self', type: 'text/html', href: `${nconf.get('url')}/${user.username}` }]
            };
            res.json(response);
        } catch (err) {
            next(err);
        }
    });

    router.get("/.well-known/change-password", (req, res) => {
        res.redirect(302, `${nconf.get('url')}/me/edit/password`);
    });
};
>>>>>>> REPLACE

### FILE: src/routes/well-known.js
<<<<<<< SEARCH
=======
const express = require('express');
const controllers = require('../controllers/well-known');

module.exports = function (router) {
    const wellKnownRouter = express.Router();
    controllers(wellKnownRouter);
    router.use('/.well-known', wellKnownRouter);
};
>>>>>>> REPLACE

### FILE: src/app.js
<<<<<<< SEARCH
const routes = require('./src/routes');
=======
const routes = require('./src/routes');
const wellKnownRoutes = require('./src/routes/well-known');
>>>>>>> REPLACE

### FILE: src/app.js
<<<<<<< SEARCH
app.use('/api', api);
app.use('/admin', admin);
app.use('/categories', categories);
app.use('/topics', topics);
app.use('/posts', posts);
app.use('/user', user);
app.use('/groups', groups);
app.use('/meta', meta);
app.use('/translator', translator);
app.use('/plugins', plugins);
app.use('/uploads', express.static(nconf.get('upload_path')));
app.use('/assets', express.static(path.join(__dirname, 'public')));
app.use('/socket.io', socketio);
=======
app.use('/api', api);
app.use('/admin', admin);
app.use('/categories', categories);
app.use('/topics', topics);
app.use('/posts', posts);
app.use('/user', user);
app.use('/groups', groups);
app.use('/meta', meta);
app.use('/translator', translator);
app.use('/plugins', plugins);
app.use('/uploads', express.static(nconf.get('upload_path')));
app.use('/assets', express.static(path.join(__dirname, 'public')));
app.use('/socket.io', socketio);
wellKnownRoutes(app);
>>>>>>> REPLACE