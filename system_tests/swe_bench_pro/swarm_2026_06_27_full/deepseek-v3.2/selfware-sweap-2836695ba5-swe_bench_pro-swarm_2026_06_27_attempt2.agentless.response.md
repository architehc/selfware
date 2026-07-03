### FILE: src/controllers/well-known.js
<<<<<<< SEARCH
=======
"use strict";

const user = require('../user');
const privileges = require('../privileges');
const helpers = require('./helpers');

module.exports = function (router, middleware) {
    router.get('/.well-known/webfinger', middleware.applyCSRF, async (req, res) => {
        const { resource } = req.query;
        if (!resource) {
            return helpers.formatApiResponse(400, res, new Error('[[error:invalid-resource]]'));
        }

        if (!resource.startsWith('acct:')) {
            return helpers.formatApiResponse(400, res, new Error('[[error:invalid-resource]]'));
        }

        const hostname = require('nconf').get('url_parsed').hostname;
        if (!resource.endsWith(`@${hostname}`)) {
            return helpers.formatApiResponse(400, res, new Error('[[error:invalid-resource]]'));
        }

        const uid = req.uid || 0;
        const canView = await privileges.global.can('groups:view:users', uid);
        if (!canView) {
            return helpers.formatApiResponse(403, res, new Error('[[error:no-privileges]]'));
        }

        const username = resource.slice(5, -(`@${hostname}`).length);
        const exists = await user.exists(username);
        if (!exists) {
            return helpers.formatApiResponse(404, res, new Error('[[error:no-user]]'));
        }

        const userData = await user.getUserDataByUsername(username);
        const slug = require('../slugify')(username);
        const aliases = [
            `${require('nconf').get('url')}/uid/${userData.uid}`,
            `${require('nconf').get('url')}/user/${slug}`,
        ];
        const links = [{
            rel: 'http://webfinger.net/rel/profile-page',
            type: 'text/html',
            href: `${require('nconf').get('url')}/user/${slug}`,
        }];

        res.status(200).json({
            subject: resource,
            aliases,
            links,
        });
    });

    router.get('/.well-known/change-password', (req, res) => {
        res.redirect(`${require('nconf').get('relative_path')}/me/edit/password`);
    });
};
>>>>>>> REPLACE

### FILE: src/routes/well-known.js
<<<<<<< SEARCH
=======
"use strict";

const wellKnownController = require('../controllers/well-known');

module.exports = function (router, middleware) {
    wellKnownController(router, middleware);
};
>>>>>>> REPLACE

### FILE: src/routes/index.js
<<<<<<< SEARCH
    app.use('/', require('./home.js')(router, middleware));
    app.use('/', require('./categories.js')(router, middleware));
    app.use('/', require('./topics.js')(router, middleware));
    app.use('/', require('./posts.js')(router, middleware));
    app.use('/', require('./users.js')(router, middleware));
    app.use('/', require('./groups.js')(router, middleware));
    app.use('/', require('./tags.js')(router, middleware));
    app.use('/', require('./search.js')(router, middleware));
    app.use('/', require('./unread.js')(router, middleware));
    app.use('/', require('./recent.js')(router, middleware));
    app.use('/', require('./popular.js')(router, middleware));
    app.use('/', require('./notifications.js')(router, middleware));
    app.use('/', require('/src/routes/chat.js')(router, middleware));
    app.use('/', require('./flags.js')(router, middleware));
    app.use('/', require('./admin.js')(router, middleware));
    app.use('/api', require('./write.js')(router, middleware));
    app.use('/api/v3', require('./api.js')(router, middleware));
=======
    app.use('/', require('./home.js')(router, middleware));
    app.use('/', require('./categories.js')(router, middleware));
    app.use('/', require('./topics.js')(router, middleware));
    app.use('/', require('./posts.js')(router, middleware));
    app.use('/', require('./users.js')(router, middleware));
    app.use('/', require('./groups.js')(router, middleware));
    app.use('/', require('./tags.js')(router, middleware));
    app.use('/', require('./search.js')(router, middleware));
    app.use('/', require('./unread.js')(router, middleware));
    app.use('/', require('./recent.js')(router, middleware));
    app.use('/', require('./popular.js')(router, middleware));
    app.use('/', require('./notifications.js')(router, middleware));
    app.use('/', require('/src/routes/chat.js')(router, middleware));
    app.use('/', require('./flags.js')(router, middleware));
    app.use('/', require('./admin.js')(router, middleware));
    app.use('/', require('./well-known.js')(router, middleware));
    app.use('/api', require('./write.js')(router, middleware));
    app.use('/api/v3', require('./api.js')(router, middleware));
>>>>>>> REPLACE

### FILE: src/routes/users.js
<<<<<<< SEARCH
    router.get('/.well-known/change-password', (req, res) => {
        res.redirect(`${nconf.get('relative_path')}/me/edit/password`);
    });
=======
>>>>>>> REPLACE