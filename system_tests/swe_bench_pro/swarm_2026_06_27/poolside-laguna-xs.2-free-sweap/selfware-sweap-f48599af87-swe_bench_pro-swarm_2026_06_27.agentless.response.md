
Looking at the issue and requirements, I need to:

1. Implement `db.mget()` for all database adapters
2. Add `User.email.getEmailForValidation()` method
3. Update `User.email.isValidationPending()` to check expires timestamp
4. Update `User.email.canSendValidation()` to use stored expires timestamp
5. Update `User.email.sendValidationEmail()` to store expires field
6. Update `User.email.expireValidation()` to be called on user deletion

Let me start with the database adapters:
### FILE: src/database/redis.js
<<<<<<< SEARCH
	db.get = function (key, callback) {
		return new Promise(function (resolve, reject) {
			client.get(key, function (err, result) {
				if (err) {
					return reject(err);
				}
				resolve(result);
			});
		});
	};
=======
	db.mget = function (keys, callback) {
		return new Promise(function (resolve, reject) {
			if (!keys || !keys.length) {
				return resolve([]);
			}
			client.mget(keys, function (err, results) {
				if (err) {
					return reject(err);
				}
				resolve(results.map(result => result || null));
			});
		});
	};

	db.get = function (key, callback) {
		return new Promise(function (resolve, reject) {
			client.get(key, function (err, result) {
				if (err) {
					return reject(err);
				}
				resolve(result);
			});
		});
	};
>>>>>>> REPLACE

### FILE: src/database/mongo.js
<<<<<<< SEARCH
	db.get = function (key, callback) {
		return new Promise(function (resolve, reject) {
			collection.findOne({ _key: key }, function (err, result) {
				if (err) {
					return reject(err);
				}
				resolve(result ? result.value : null);
			});
		});
	};
=======
	db.mget = function (keys, callback) {
		return new Promise(function (resolve, reject) {
			if (!keys || !keys.length) {
				return resolve([]);
			}
			collection.find({ _key: { $in: keys } }).toArray(function (err, results) {
				if (err) {
					return reject(err);
				}
				const keyToValue = {};
				results.forEach(function (result) {
					keyToValue[result._key] = result.value;
				});
				resolve(keys.map(key => keyToValue[key] || null));
			});
		});
	};

	db.get = function (key, callback) {
		return new Promise(function (resolve, reject) {
			collection.findOne({ _key: key }, function (err, result) {
				if (err) {
					return reject(err);
				}
				resolve(result ? result.value : null);
			});
		});
	};
>>>>>>> REPLACE

### FILE: src/database/postgres.js
<<<<<<< SEARCH
	db.get = function (key, callback) {
		return new Promise(function (resolve, reject) {
			pgClient.query('SELECT value FROM legacy_object_live WHERE key = $1', [key], function (err, result) {
				if (err) {
					return reject(err);
				}
				resolve(result.rows.length ? result.rows[0].value : null);
			});
		});
	};
=======
	db.mget = function (keys, callback) {
		return new Promise(function (resolve, reject) {
			if (!keys || !keys.length) {
				return resolve([]);
			}
			pgClient.query('SELECT key, value FROM legacy_object_live WHERE key = ANY($1)', [keys], function (err, result) {
				if (err) {
					return reject(err);
				}
				const keyToValue = {};
				result.rows.forEach(function (row) {
					keyToValue[row.key] = row.value;
				});
				resolve(keys.map(key => keyToValue[key] || null));
			});
		});
	};

	db.get = function (key, callback) {
		return new Promise(function (resolve, reject) {
			pgClient.query('SELECT value FROM legacy_object_live WHERE key = $1', [key], function (err, result) {
				if (err) {
					return reject(err);
				}
				resolve(result.rows.length ? result.rows[0].value : null);
			});
		});
	};
>>>>>>> REPLACE

Now let me update the email.js file:
### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.isValidationPending = async (uid, email) => {
	const code = await db.get(`confirm:byUid:${uid}`);

	if (email) {
		const confirmObj = await db.getObject(`confirm:${code}`);
		return !!(confirmObj && email === confirmObj.email);
	}

	return !!code;
};
=======
UserEmail.getEmailForValidation = async (uid) => {
	const email = await user.getUserField(uid, 'email');
	if (email) {
		return email;
	}

	const code = await db.get(`confirm:byUid:${uid}`);
	if (!code) {
		return null;
	}

	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj || confirmObj.uid !== uid) {
		return null;
	}

	return confirmObj.email;
};

UserEmail.isValidationPending = async (uid, email) => {
	const code = await db.get(`confirm:byUid:${uid}`);

	if (!code) {
		return false;
	}

	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj) {
		return false;
	}

	const now = Date.now();
	if (confirmObj.expires && now >= confirmObj.expires) {
		return false;
	}

	if (email) {
		return email === confirmObj.email;
	}

	return true;
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.getValidationExpiry = async (uid) => {
	const pending = await UserEmail.isValidationPending(uid);
	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
};
=======
UserEmail.getValidationExpiry = async (uid) => {
	const pending = await UserEmail.isValidationPending(uid);
	if (!pending) {
		return null;
	}

	const code = await db.get(`confirm:byUid:${uid}`);
	if (!code) {
		return null;
	}

	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj || !confirmObj.expires) {
		return null;
	}

	return confirmObj.expires - Date.now();
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.canSendValidation = async (uid, email) => {
	const pending = await UserEmail.isValidationPending(uid, email);
	if (!pending) {
		return true;
	}

	const ttl = await UserEmail.getValidationExpiry(uid);
	const max = meta.config.emailConfirmExpiry * 60 * 60 * 1000;
	const interval = meta.config.emailConfirmInterval * 60 * 1000;

	return ttl + interval < max;
};
=======
UserEmail.canSendValidation = async (uid, email) => {
	const pending = await UserEmail.isValidationPending(uid, email);
	if (!pending) {
		return true;
	}

	const ttl = await UserEmail.getValidationExpiry(uid);
	const max = meta.config.emailConfirmExpiry * 60 * 60 * 1000;
	const interval = meta.config.emailConfirmInterval * 60 * 1000;

	if (ttl === null) {
		return true;
	}

	return ttl + interval < max;
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
	await UserEmail.expireValidation(uid);
	await db.set(`confirm:byUid:${uid}`, confirm_code);
	await db.pexpire(`confirm:byUid:${uid}`, emailConfirmExpiry * 60 * 60 * 1000);

	await db.setObject(`confirm:${confirm_code}`, {
		email: options.email.toLowerCase(),
		uid: uid,
	});
	await db.pexpire(`confirm:${confirm_code}`, emailConfirmExpiry * 60 * 60 * 1000);
=======
	await UserEmail.expireValidation(uid);
	await db.set(`confirm:byUid:${uid}`, confirm_code);

	const expires = Date.now() + (emailConfirmExpiry * 60 * 60 * 1000);
	await db.setObject(`confirm:${confirm_code}`, {
		email: options.email.toLowerCase(),
		uid: uid,
		expires: expires,
	});
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.confirmByUid = async function (uid) {
	if (!(parseInt(uid, 10) > 0)) {
		throw new Error('[[error:invalid-uid]]');
	}
	const currentEmail = await user.getUserField(uid, 'email');
	if (!currentEmail) {
		throw new Error('[[error:invalid-email]]');
	}
=======
UserEmail.validateEmail = async function (uid) {
	const email = await UserEmail.getEmailForValidation(uid);
	if (!email) {
		throw new Error('[[error:invalid-email]]');
	}

	await user.setUserField(uid, 'email', email);
	await UserEmail.confirmByUid(uid);
};

UserEmail.confirmByUid = async function (uid) {
	if (!(parseInt(uid, 10) > 0)) {
		throw new Error('[[error:invalid-uid]]');
	}
	const currentEmail = await user.getUserField(uid, 'email');
	if (!currentEmail) {
		const email = await UserEmail.getEmailForValidation(uid);
		if (!email) {
			throw new Error('[[error:invalid-email]]');
		}
		await user.setUserField(uid, 'email', email);
	}
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.sendValidationEmail = async function (uid, options) {
	/*
	 * Options:
	 * - email, overrides email retrieval
	 * - force, sends email even if it is too soon to send another
	 * - template, changes the template used for email sending
	 */

	if (meta.config.sendValidationEmail !== 1) {
		winston.verbose(`[user/email] Validation email for uid ${uid} not sent due to config settings`);
		return;
	}

	options = options || {};

	// Fallback behaviour (email passed in as second argument)
	if (typeof options === 'string') {
		options = {
			email: options,
		};
	}

	const confirm_code = utils.generateUUID();
	const confirm_link = `${nconf.get('url')}/confirm/${confirm_code}`;

	const { emailConfirmInterval, emailConfirmExpiry } = meta.config;

	// If no email passed in (default), retrieve email from uid
	if (!options.email || !options.email.length) {
		options.email = await user.getUserField(uid, 'email');
	}
	if (!options.email) {
		return;
	}
=======
UserEmail.sendValidationEmail = async function (uid, options) {
	/*
	 * Options:
	 * - email, overrides email retrieval
	 * - force, sends email even if it is too soon to send another
	 * - template, changes the template used for email sending
	 */

	if (meta.config.sendValidationEmail !== 1) {
		winston.verbose(`[user/email] Validation email for uid ${uid} not sent due to config settings`);
		return;
	}

	options = options || {};

	// Fallback behaviour (email passed in as second argument)
	if (typeof options === 'string') {
		options = {
			email: options,
		};
	}

	const confirm_code = utils.generateUUID();
	const confirm_link = `${nconf.get('url')}/confirm/${confirm_code}`;

	const { emailConfirmInterval, emailConfirmExpiry } = meta.config;

	// If no email passed in (default), retrieve email from uid
	if (!options.email || !options.email.length) {
		options.email = await UserEmail.getEmailForValidation(uid);
	}
	if (!options.email) {
		return;
	}
>>>>>>> REPLACE

Now let me update the user deletion to call expireValidation:
### FILE: src/api/users.js
<<<<<<< SEARCH
async function processDeletion({ uid: uid, method, password, caller }) {
	const userData = await user.getUserData(uid);
	if (!userData) {
		throw new Error('[[error:invalid-uid]]');
	}

	const [isAdmin, isGlobalMod] = await Promise.all([
		user.isAdministrator(caller.uid),
		user.isGlobalModerator(caller.uid),
	]);

	if (!isAdmin && !isGlobalMod) {
		throw new Error('[[error:no-privileges]]');
	}

	if (method === 'delete') {
		await user.delete(uid);
	} else if (method === 'deleteContent') {
		await user.delete(uid, true);
	} else if (method === 'deleteAccount') {
		await user.delete(uid, true);
		await user.email.remove(uid);
	}

	await events.log({
		type: `user-${method}`,
		uid: caller.uid,
		targetUid: uid,
		ip: caller.ip,
		username: userData.username,
		email: userData.email,
	});
}
=======
async function processDeletion({ uid: uid, method, password, caller }) {
	const userData = await user.getUserData(uid);
	if (!userData) {
		throw new Error('[[error:invalid-uid]]');
	}

	const [isAdmin, isGlobalMod] = await Promise.all([
		user.isAdministrator(caller.uid),
		user.isGlobalModerator(caller.uid),
	]);

	if (!isAdmin && !isGlobalMod) {
		throw new Error('[[error:no-privileges]]');
	}

	await user.email.expireValidation(uid);

	if (method === 'delete') {
		await user.delete(uid);
	} else if (method === 'deleteContent') {
		await user.delete(uid, true);
	} else if (method === 'deleteAccount') {
		await user.delete(uid, true);
		await user.email.remove(uid);
	}

	await events.log({
		type: `user-${method}`,
		uid: caller.uid,
		targetUid: uid,
		ip: caller.ip,
		username: userData.username,
		email: userData.email,
	});
}
>>>>>>> REPLACE
