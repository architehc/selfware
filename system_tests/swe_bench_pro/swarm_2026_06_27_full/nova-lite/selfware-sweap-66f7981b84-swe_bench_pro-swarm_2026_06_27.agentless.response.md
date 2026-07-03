### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.sendValidationEmail = async function (uid, options) {
	...
	const confirm_code = utils.generateUUID();
	const confirm_link = `${nconf.get('url')}/confirm/${confirm_code}`;

	const { emailConfirmInterval, emailConfirmExpiry } = meta.config;

	if (!options.email ||!options.email.length) {
		options.email = await user.getUserField(uid, 'email');
	}
	if (!options.email) {
		return;
	}

	if (!options.force &&!await UserEmail.canSendValidation(uid, options.email)) {
		throw new Error(`[[error:confirm-email-already-sent, ${emailConfirmInterval}]]`);
	}

	const username = await user.getUserField(uid, 'username');
	const data = await plugins.hooks.fire('filter:user.verify', {
		uid,
		username,
		confirm_link,
		confirm_code: await plugins.hooks.fire('filter:user.verify.code', confirm_code),
		email: options.email,

		subject: options.subject || '[[email:email.verify-your-email.subject]]',
		template: options.template || 'verify-email',
	});

	await UserEmail.expireValidation(uid);
	await db.set(`confirm:byUid:${uid}`, confirm_code);
	await db.pexpire(`confirm:byUid:${uid}`, emailConfirmExpiry * 60 * 60 * 1000);

	await db.setObject(`confirm:${confirm_code}`, {
		email: options.email.toLowerCase(),
		uid: uid,
		expires: Date.now() + emailConfirmExpiry * 60 * 60 * 1000
	});
	await db.pexpire(`confirm:${confirm_code}`, emailConfirmExpiry * 60 * 60 * 1000);

	...
};
=======
UserEmail.sendValidationEmail = async function (uid, options) {
	...
	const confirm_code = utils.generateUUID();
	const confirm_link = `${nconf.get('url')}/confirm/${confirm_code}`;

	const { emailConfirmInterval, emailConfirmExpiry } = meta.config;

	if (!options.email ||!options.email.length) {
		options.email = await user.getUserField(uid, 'email');
	}
	if (!options.email) {
		return;
	}

	if (!options.force &&!await UserEmail.canSendValidation(uid, options.email)) {
		throw new Error(`[[error:confirm-email-already-sent, ${emailConfirmInterval}]]`);
	}

	const username = await user.getUserField(uid, 'username');
	const data = await plugins.hooks.fire('filter:user.verify', {
		uid,
		username,
		confirm_link,
		confirm_code: await plugins.hooks.fire('filter:user.verify.code', confirm_code),
		email: options.email,

		subject: options.subject || '[[email:email.verify-your-email.subject]]',
		template: options.template || 'verify-email',
	});

	await UserEmail.expireValidation(uid);
	await db.set(`confirm:byUid:${uid}`, confirm_code);
	await db.pexpire(`confirm:byUid:${uid}`, emailConfirmExpiry * 60 * 60 * 1000);

	await db.setObject(`confirm:${confirm_code}`, {
		email: options.email.toLowerCase(),
		uid: uid,
		expires: Date.now() + emailConfirmExpiry * 60 * 60 * 1000
	});
	await db.pexpire(`confirm:${confirm_code}`, emailConfirmExpiry * 60 * 60 * 1000);

	...
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.confirmByCode = async function (code, sessionId) {
	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj ||!confirmObj.uid ||!confirmObj.email) {
		throw new Error('[[error:invalid-data]]');
	}

	const now = Date.now();
	if (now > confirmObj.expires) {
		throw new Error('[[error:expired-code]]');
	}

	const oldUid = await db.sortedSetScore('email:uid', confirmObj.email.toLowerCase());
	if (oldUid) {
		await UserEmail.remove(oldUid, sessionId);
	}

	const oldEmail = await user.getUserField(confirmObj.uid, 'email');
	if (oldEmail && confirmObj.email!== oldEmail) {
		await UserEmail.remove(confirmObj.uid, sessionId);
	} else {
		await user.auth.revokeAllSessions(confirmObj.uid, sessionId);
	}

	await user.setUserField(confirmObj.uid, 'email', confirmObj.email);
	await Promise.all([
		UserEmail.confirmByUid(confirmObj.uid),
		db.delete(`confirm:${code}`),
		events.log({ type: 'email-change', oldEmail, newEmail: confirmObj.email }),
	]);
};
=======
UserEmail.confirmByCode = async function (code, sessionId) {
	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj ||!confirmObj.uid ||!confirmObj.email) {
		throw new Error('[[error:invalid-data]]');
	}

	const now = Date.now();
	if (now > confirmObj.expires) {
		throw new Error('[[error:expired-code]]');
	}

	const oldUid = await db.sortedSetScore('email:uid', confirmObj.email.toLowerCase());
	if (oldUid) {
		await UserEmail.remove(oldUid, sessionId);
	}

	const oldEmail = await user.getUserField(confirmObj.uid, 'email');
	if (oldEmail && confirmObj.email!== oldEmail) {
		await UserEmail.remove(confirmObj.uid, sessionId);
	} else {
		await user.auth.revokeAllSessions(confirmObj.uid, sessionId);
	}

	await user.setUserField(confirmObj.uid, 'email', confirmObj.email);
	await Promise.all([
		UserEmail.confirmByUid(confirmObj.uid),
		db.delete(`confirm:${code}`),
		events.log({ type: 'email-change', oldEmail, newEmail: confirmObj.email }),
	]);
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.getEmailForValidation = async function (uid) {
	const email = await user.getUserField(uid, 'email');
	if (email) {
		return email;
	}

	const code = await db.get(`confirm:byUid:${uid}`);
	if (!code) {
		return null;
	}

	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj ||!confirmObj.email) {
		return null;
	}

	return confirmObj.email;
};
=======
UserEmail.getEmailForValidation = async function (uid) {
	const email = await user.getUserField(uid, 'email');
	if (email) {
		return email;
	}

	const code = await db.get(`confirm:byUid:${uid}`);
	if (!code) {
		return null;
	}

	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj ||!confirmObj.email) {
		return null;
	}

	return confirmObj.email;
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.isValidationPending = async (uid, email) => {
	const code = await db.get(`confirm:byUid:${uid}`);

	if (email) {
		const confirmObj = await db.getObject(`confirm:${code}`);
		return!!(confirmObj && email === confirmObj.email);
	}

	return!!code;
};
=======
UserEmail.isValidationPending = async (uid, email) => {
	const code = await db.get(`confirm:byUid:${uid}`);

	if (email) {
		const confirmObj = await db.getObject(`confirm:${code}`);
		return!!(confirmObj && email === confirmObj.email && Date.now() < confirmObj.expires);
	}

	return!!code;
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

	return Date.now() - ttl > interval;
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.getValidationExpiry = async (uid) => {
	const pending = await UserEmail.isValidationPending(uid);
	return pending? db.pttl(`confirm:byUid:${uid}`) : null;
};
=======
UserEmail.getValidationExpiry = async (uid) => {
	const code = await db.get(`confirm:byUid:${uid}`);
	if (!code) {
		return null;
	}

	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj ||!confirmObj.expires) {
		return null;
	}

	return confirmObj.expires - Date.now();
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.expireValidation = async (uid) => {
	const code = await db.get(`confirm:byUid:${uid}`);
	await db.deleteAll([
		`confirm:byUid:${uid}`,
		`confirm:${code}`,
	]);
};
=======
UserEmail.expireValidation = async (uid) => {
	const code = await db.get(`confirm:byUid:${uid}`);
	if (code) {
		await db.delete(`confirm:byUid:${uid}`);
		await db.deleteObject(`confirm:${code}`);
	}
};
>>>>>>> REPLACE

### FILE: src/database/mongodb.js
<<<<<<< SEARCH
db.mget = async function (keys) {
	const results = await db.client.collection('objects').find({
		_key: { $in: keys }
	}).toArray();

	return keys.map(key => {
		const obj = results.find(result => result._key === key);
		return obj? obj.value : null;
	});
};
=======
db.mget = async function (keys) {
	const results = await db.client.collection('objects').find({
		_key: { $in: keys }
	}).toArray();

	return keys.map(key => {
		const obj = results.find(result => result._key === key);
		return obj? obj.value : null;
	});
};
>>>>>>> REPLACE

### FILE: src/database/postgresql.js
<<<<<<< SEARCH
db.mget = async function (keys) {
	const query = `
		SELECT value
		FROM legacy_object_live
		JOIN legacy_string ON legacy_object_live.value_id = legacy_string.id
		WHERE legacy_object_live._key IN (:keys)
	`;

	const values = { keys: keys.map(key => `'${key}'`).join(',') };
	const results = await db.client.query(query, values);

	return keys.map(key => {
		const result = results.rows.find(row => row.value === key);
		return result? result.value : null;
	});
};
=======
db.mget = async function (keys) {
	const query = `
		SELECT value
		FROM legacy_object_live
		JOIN legacy_string ON legacy_object_live.value_id = legacy_string.id
		WHERE legacy_object_live._key IN (:keys)
	`;

	const values = { keys: keys.map(key => `'${key}'`).join(',') };
	const results = await db.client.query(query, values);

	return keys.map(key => {
		const result = results.rows.find(row => row.value === key);
		return result? result.value : null;
	});
};
>>>>>>> REPLACE

### FILE: src/database/redis.js
<<<<<<< SEARCH
db.mget = async function (keys) {
	const values = await db.client.mget(keys);
	return values.map((value, index) => value === null? null : value);
};
=======
db.mget = async function (keys) {
	const values = await db.client.mget(keys);
	return values.map((value, index) => value === null? null : value);
};
>>>>>>> REPLACE

### FILE: test/database/keys.js
<<<<<<< SEARCH
it('should return multiple keys and null if key doesn\'t exist', async () => {
	const data = await db.mget(['doesnotexist', 'testKey']);
	assert.deepStrictEqual(data, [null, 'testValue']);
});

it('should return empty array if keys is empty array or falsy', async () => {
	assert.deepStrictEqual(await db.mget([]), []);
	assert.deepStrictEqual(await db.mget(false), []);
	assert.deepStrictEqual(await db.mget(null), []);
});
=======
it('should return multiple keys and null if key doesn\'t exist', async () => {
	const data = await db.mget(['doesnotexist', 'testKey']);
	assert.deepStrictEqual(data, [null, 'testValue']);
});

it('should return empty array if keys is empty array or falsy', async () => {
	assert.deepStrictEqual(await db.mget([]), []);
	assert.deepStrictEqual(await db.mget(false), []);
	assert.deepStrictEqual(await db.mget(null), []);
});
>>>>>>> REPLACE

### FILE: test/user/emails.js
<<<<<<< SEARCH
it('should return true if key exist', async () => {
	const data = await db.mget(['testKey']);
	assert.deepStrictEqual(data, ['testValue']);
});
=======
it('should return true if key exist', async () => {
	const data = await db.mget(['testKey']);
	assert.deepStrictEqual(data, ['testValue']);
});
>>>>>>> REPLACE

### FILE: test/user/emails.js
<<<<<<< SEARCH
it('should return true if it has been long enough to re-send confirmation', async () => {
	const email = 'test@example.org';
	await user.email.sendValidationEmail(uid, {
		email,
	});
	const code = await db.get(`confirm:byUid:${uid}`);
	await db.setObjectField(`confirm:${code}`, 'expires', Date.now() + 1000);
	const ok = await user.email.canSendValidation(uid, email);
	assert(ok);
});
=======
it('should return true if it has been long enough to re-send confirmation', async () => {
	const email = 'test@example.org';
	await user.email.sendValidationEmail(uid, {
		email,
	});
	const code = await db.get(`confirm:byUid:${uid}`);
	await db.setObjectField(`confirm:${code}`, 'expires', Date.now() + 1000);
	const ok = await user.email.canSendValidation(uid, email);
	assert(ok);
});
>>>>>>> REPLACE