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

			it('should return a valid webfinger response if the user exists', async () => {
				const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger?resource=acct:${username}@${nconf.get('url_parsed').hostname}`, {
					json: true,
					simple: false,
					resolveWithFullResponse: true,
				});

				assert.strictEqual(response.statusCode, 200);
				assert.deepEqual(response.body, {
					subject: `acct:${username}@${nconf.get('url_parsed').hostname}`,
					aliases: [
						`acct:${username}@${nconf.get('url_parsed').hostname}`,
						`/user/${username}`,
					],
					links: [
						{
							rel: 'self',
							type: 'application/activity+json',
							href: `/user/${username}`,
						},
					],
				});
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
				assert.deepEqual(response.body, {
					subject: `acct:${username}@${nconf.get('url_parsed').hostname}`,
					aliases: [
						`acct:${username}@${nconf.get('url_parsed').hostname}`,
						`/user/${username}`,
					],
					links: [
						{
							rel: 'self',
							type: 'application/activity+json',
							href: `/user/${username}`,
						},
					],
				});
			});
>>>>>>> REPLACE

### FILE: src/routes/user.js
<<<<<<< SEARCH
	router.get('/me/edit/password', requireAuthentication, user.editPassword);
=======
	// Removed user-specific route for change-password

>>>>>>> REPLACE

### FILE: src/controllers/well-known.js
<<<<<<< SEARCH
=======
"use strict";

const nconf = require('nconf');
const user = require('../src/user');
const privileges = require('../src/privileges');
const utils = require('../src/utils');

module.exports = function (router) {
	router.get("/.well-known/webfinger", webfingerHandler);
	router.get("/.well-known/change-password", (req, res) => {
		res.redirect('/me/edit/password');
	});
};

async function webfingerHandler(req, res) {
	const resource = req.query.resource;

	if (!resource) {
		return res.status(400).send('Missing resource parameter');
	}

	if (!resource.startsWith('acct:') || !resource.endsWith('@' + nconf.get('url_parsed').hostname)) {
		return res.status(400).send('Malformed resource parameter');
	}

	const username = resource.substring(5, resource.length - (nconf.get('url_parsed').hostname.length + 1));

	let uid;
	try {
		uid = await user.getByUsername(username);
	} catch (e) {
		return res.status(404).send('User not found');
	}

	if (!uid) {
		return res.status(404).send('User not found');
	}

	let authorized = true;
	if (req.uid) {
		try {
			authorized = await privileges.check(req.uid, 'groups:view:users');
		} catch (e) {
			authorized = false;
		}
	} else {
		try {
			authorized = await privileges.check('guests', 'groups:view:users');
		} catch (e) {
			authorized = false;
		}
	}

	if (!authorized) {
		return res.status(403).send('Forbidden');
	}

	const webfinger = {
		subject: resource,
		aliases: [
			resource,
			`/user/${username}`,
		],
		links: [
			{
				rel: 'self',
				type: 'application/activity+json',
				href: `/user/${username}`,
			},
		],
	};

	res.status(200).json(webfinger);
}
>>>>>>> REPLACE
