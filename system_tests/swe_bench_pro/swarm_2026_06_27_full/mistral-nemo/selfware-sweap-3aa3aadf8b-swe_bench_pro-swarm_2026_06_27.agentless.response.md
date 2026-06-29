### FILE: test/controllers.js
<<<<<<< SEARCH
it('should respond appropriately if the user requested does not exist locally', async () => {
	const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger?resource=acct:foobar@${nconf.get('url_parsed').hostname}`, {
		json: true,
		simple: false,
		resolveWithFullResponse: true,
	});

	assert.strictEqual(response.statusCode, 404);
});
=======
it('should respond appropriately if the user requested does not exist locally', async () => {
	const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger?resource=acct:foobar@${nconf.get('url_parsed').hostname}`, {
		json: true,
		simple: false,
		resolveWithFullResponse: true,
	});

	assert.strictEqual(response.statusCode, 404);
});

it('should return a valid webfinger response if the user exists', async () => {
	const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger?resource=acct:${username}@${nconf.get('url_parsed').hostname}`, {
		json: true,
		simple: false,
		resolveWithFullResponse: true,
	});

	assert.strictEqual(response.statusCode, 200);
	const body = JSON.parse(response.body);
	assert.strictEqual(body.subject, `acct:${username}@${nconf.get('url_parsed').hostname}`);
	assert(body.aliases.includes(`https://${nconf.get('url_parsed').hostname}/user/${username}`));
	assert(body.links.length > 0);
});
>>>>>>> REPLACE

### FILE: src/controllers/well-known.js
<<<<<<< SEARCH
=======
"use strict";

const { parse } = require('url');

module.exports = function (router) {
	router.get("/.well-known/webfinger", async (req, res, next) => {
		const { resource } = req.query;
		if (!resource || !resource.startsWith('acct:')) {
			return res.status(400).send('Bad Request');
		}

		const [, username, domain] = resource.match(/^acct:([^@]+)@(.+)$/);
		if (!username || !domain) {
			return res.status(400).send('Bad Request');
		}

		if (domain !== nconf.get('url_parsed').hostname) {
			return res.status(404).send('Not Found');
		}

		const user = await user.getUserByUsername(username);
		if (!user) {
			return res.status(404).send('Not Found');
		}

		const links = [
			{
				rel: 'http://schema.org/ProfilePage',
				type: 'text/html',
				href: `${nconf.get('url')}/user/${username}`,
			},
		];

		const response = {
			subject: `acct:${username}@${domain}`,
			aliases: [
				`https://${domain}/user/${username}`,
				`acct:${username}@${domain}`,
			],
			links,
		};

		res.json(response);
	});
};
>>>>>>> REPLACE