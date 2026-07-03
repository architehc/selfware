
Looking at the issue, I need to create a new controller file for `.well-known` routes and a corresponding router file. Let me create these files:

### FILE: src/controllers/well-known.js
<<<<<<< SEARCH
=======
'use strict';

const nconf = require('nconf');
const user = require('../user');
const privileges = require('../privileges');

const WellKnown = module.exports;

WellKnown.webfinger = async function (req, res) {
	const resource = req.query.resource;

	// Validate resource parameter
	if (!resource) {
		return res.status(400).json({
			error: 'Bad Request',
			message: 'Missing resource parameter',
		});
	}

	if (!resource.startsWith('acct:')) {
		return res.status(400).json({
			error: 'Bad Request',
			message: 'Resource must start with acct:',
		});
	}

	const hostname = nconf.get('url_parsed').hostname;
	if (!resource.endsWith(hostname)) {
		return res.status(400).json({
			error: 'Bad Request',
			message: 'Resource must end with the expected hostname',
		});
	}

	// Check authorization
	const uid = req.uid;
	const hasPermission = await privileges.global.has('groups:view:users', uid ? [uid] : []);

	if (!hasPermission) {
		return res.status(403).json({
			error: 'Forbidden',
			message: 'Insufficient permissions',
		});
	}

	// Extract username from resource (acct:username@hostname)
	const atIndex = resource.indexOf('@');
	if (atIndex === -1) {
		return res.status(400).json({
			error: 'Bad Request',
			message: 'Invalid resource format',
		});
	}
	const username = resource.slice(4, atIndex);

	// Find user
	const userData = await user.getByUsername(username);
	if (!userData) {
		return res.status(404).json({
			error: 'Not Found',
			message: 'User not found',
		});
	}

	// Build response
	const response = {
		subject: resource,
		aliases: [
			`/user/${userData.slug}`,
			`/uid/${userData.uid}`,
		],
		links: [
			{
				rel: 'self',
				type: 'text/html',
				href: `/user/${userData.slug}`,
			},
		],
	};

	res.json(response);
};

WellKnown.changePassword = function (req, res) {
	res.redirect('/me/edit/password');
};
>>>>>>> REPLACE

### FILE: src/routes/well-known.js
<<<<<<< SEARCH
=======
'use strict';

const express = require('express');
const wellKnownController = require('../controllers/well-known');

const router = express.Router();

router.get('/webfinger', wellKnownController.webfinger);
router.get('/change-password', wellKnownController.changePassword);

module.exports = router;
>>>>>>> REPLACE
