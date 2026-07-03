### FILE: src/controllers/well-known.js
<<<<<<< SEARCH
=======

"use strict";

const { Controller } = require("engine");
const nconf = require("nconf");
const url = require("url");
const user = require("lib/user");
const privileges = require("lib/privileges");

class WellKnownController extends Controller {
    async webfinger(req, res) {
        const { resource } = req.query;

        // Validate resource parameter
        if (!resource) {
            return res.status(400).json({ error: "Missing resource parameter" });
        }

        const parsed = url.parse(`acct:${resource}`, true);
        const hostname = nconf.get("url_parsed").hostname;
        if (!parsed.hostname || parsed.hostname !== hostname) {
            return res.status(400).json({ error: "Malformed resource parameter" });
        }

        const username = parsed.pathname.slice(5); // remove "acct:"
        const userObj = await user.getUserByUsername(username);
        if (!userObj) {
            return res.status(404).json({ error: "User not found" });
        }

        // Authorization check
        const reqUid = req.uid || null;
        const guestRole = "guests";
        const hasPrivilege = await privileges.global.has("groups:view:users", reqUid || guestRole);
        if (!hasPrivilege) {
            return res.status(403).json({ error: "Forbidden" });
        }

        // Build WebFinger response
        const aliases = [
            `http://${nconf.get("url")}/user/${userObj.slug}`,
            `http://${nconf.get("url")}/u/${userObj.uid}`
        ];
        const links = [
            { rel: "http://webfinger.net/resource", type: "text/html", href: `http://${nconf.get("url")}/user/${userObj.slug}` }
        ];

        res.json({
            subject: `acct:${username}@${hostname}`,
            aliases,
            links
        });
    }

    async changePassword(req, res) {
        // Redirect to /me/edit/password
        res.redirect(`${nconf.get("url")}/me/edit/password`);
    }
}

module.exports = function (router) {
    const wellKnown = new WellKnownController();
    router.get("/.well-known/webfinger", wellKnown.webfinger.bind(wellKnown));
    router.get("/.well-known/change-password", wellKnown.changePassword.bind(wellKnown));
};

>>>>>>> REPLACE
### FILE: loader.js
<<<<<<< SEARCH
Loader.start = function () {
=======
Loader.start = function () {
    require("./controllers/well-known"); // Add new well-known routes
    // Existing start logic...
>>>>>>> REPLACE
    numProcs = getPorts().length;
    console.log(`Clustering enabled: Spinning up ${numProcs} process(es).\n`);

    for (let x = 0; x < numProcs; x += 1) {
        forkWorker(x, x === 0);
    }
};