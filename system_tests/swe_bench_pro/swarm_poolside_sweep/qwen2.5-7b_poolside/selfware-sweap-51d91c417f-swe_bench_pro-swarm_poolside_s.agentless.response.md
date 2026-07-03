### FILE: src/controllers/well-known.js
<<<<<<< SEARCH
=======
"use strict";

module.exports = function (router) {
    router.get("/.well-known/webfinger", (req, res) => {
        const resource = req.query.resource;
        if (!resource || !resource.startsWith("acct:") || !resource.endsWith(`@${nconf.get('url_parsed').hostname}`)) {
            return res.status(400).json({ error: "Invalid resource parameter" });
        }

        const username = resource.slice(5);
        const user = yield user.get(username);
        if (!user) {
            return res.status(404).json({ error: "User not found" });
        }

        if (!req.uid || !req.privileges['groups:view:users']) {
            return res.status(403).json({ error: "Forbidden" });
        }

        const subject = `acct:${username}@${nconf.get('url_parsed').hostname}`;
        const aliases = [`${nconf.get('base_url')}/user/${user.username}`, `${nconf.get('base_url')}/user/${user.username}/profile`];
        const links = [{ rel: "self", type: "application/xrd+xml", href: `${nconf.get('base_url')}/.well-known/webfinger?resource=${resource}` }];

        res.json({
            subject,
            aliases,
            links,
        });
    });

    router.get("/.well-known/change-password", (req, res) => {
        res.redirect(`${nconf.get('base_url')}/me/edit/password`);
    });
};
>>>>>>> REPLACE

### FILE: src/routes/well-known.js
<<<<<<< SEARCH
=======
const wellKnownController = require('../controllers/well-known');

module.exports = function (router) {
    router.get("/.well-known/webfinger", wellKnownController.webfinger);
    router.get("/.well-known/change-password", wellKnownController.changePassword);
};
>>>>>>> REPLACE