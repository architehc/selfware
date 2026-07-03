### FILE: test/controllers.js
<<<<<<< SEARCH
it('should error if resource parameter is missing', async () => {
	const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger`, {
		json: true,
		simple: false,
		resolveWithFullResponse: true,
	});
>>>>>>> REPLACE
it('should error if resource parameter is missing', async () => {
	const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger?resource=acct:${username}@${nconf.get('url_parsed').hostname}`, {
		json: true,
		simple: false,
		resolveWithFullResponse: true,
	});
<<<<<<< SEARCH
assert.strictEqual(response.statusCode, 400);
>>>>>>> REPLACE
});

### FILE: test/controllers.js
<<<<<<< SEARCH
it('should error if resource parameter is malformed', async () => {
	const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger?resource=foobar`, {
		json: true,
		simple: false,
		resolveWithFullResponse: true,
	});
>>>>>>> REPLACE
it('should error if resource parameter is malformed', async () => {
	const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger?resource=acct:foobar@${nconf.get('url_parsed').hostname}`, {
		json: true,
		simple: false,
		resolveWithFullResponse: true,
	});
<<<<<<< SEARCH
assert.strictEqual(response.statusCode, 400);
>>>>>>> REPLACE

### FILE: test/controllers.js
<<<<<<< SEARCH
it('should deny access if view:users privilege is not enabled for guests', async () => {
	await privileges.global.rescind(['groups:view:users'], 'guests');
>>>>>>> REPLACE
it('should deny access if view:users privilege is not enabled for guests', async () => {
	await privileges.global.rescind(['groups:view:users'], 'guests');
	const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger?resource=acct:${username}@${nconf.get('url_parsed').hostname}`, {
		json: true,
		simple: false,
		resolveWithFullResponse: true,
	});
<<<<<<< SEARCH
assert.strictEqual(response.statusCode, 403);
>>>>>>> REPLACE

### FILE: test/controllers.js
<<<<<<< SEARCH
it('should respond appropriately if the user requested does not exist locally', async () => {
	const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger?resource=acct:foobar@${nconf.get('url_parsed').hostname}`, {
		json: true,
		simple: false,
		resolveWithFullResponse: true,
	});
>>>>>>> REPLACE
it('should respond appropriately if the user requested does not exist locally', async () => {
	const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger?resource=acct:nonexistent@${nconf.get('url_parsed').hostname}`, {
		json: true,
		simple: false,
		resolveWithFullResponse: true,
	});
<<<<<<< SEARCH
assert.strictEqual(response.statusCode, 404);
>>>>>>> REPLACE

### FILE: test/controllers.js
<<<<<<< SEARCH
it('should return a valid webfinger response if the user exists', async () => {
	const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger?resource=acct:${username}@${nconf.get('url_parsed').hostname}`, {
		json: true,
		simple: false,
		resolveWithFullResponse: true,
	});
>>>>>>> REPLACE
it('should return a valid webfinger response if the user exists', async () => {
	const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger?resource=acct:${username}@${nconf.get('url_parsed').hostname}`, {
		json: true,
		simple: false,
		resolveWithFullResponse: true,
	});
<<<<<<< SEARCH
assert.st
>>>>>>> REPLACE

### FILE: src/controllers/well-known.js
<<<<<<< SEARCH
>>>>>>> REPLACE
"use strict";

module.exports = function (router) {
	router.get("/.well-known/webfinger", function (req, res, next) {
		const resource = req.query.resource;
		if (!resource) {
			return res.status(400).json({ error: "Missing resource parameter" });
		}
		const username = resource.split(":")[1];
		const hostname = resource.split("@")[1];
		if (!username ||!hostname) {
			return res.status(400).json({ error: "Invalid resource parameter" });
		}
		const user = await User.getUserField(username, "uid");
		if (!user) {
			return res.status(404).json({ error: "User not found" });
		}
		const privileges = await privileges.getPrivilegesForUser(user.uid);
		if (!privileges.hasGlobalPermission("groups:view:users")) {
			return res.status(403).json({ error: "Forbidden" });
		}
		const links = [
			{
				rel: "http://webfinger.net/rel/profile-page",
				type: "text/html",
				href: `/${username}`,
			},
		];
		const aliases = [
			`acct:${username}@${hostname}`,
			`/${username}`,
		];
		res.json({
			subject: resource,
			aliases: aliases,
			links: links,
		});
	});
};