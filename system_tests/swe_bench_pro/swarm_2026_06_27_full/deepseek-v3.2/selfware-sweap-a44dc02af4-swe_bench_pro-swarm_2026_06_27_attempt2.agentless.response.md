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
UserEmail.isValidationPending = async (uid, email) => {
	const code = await db.get(`confirm:byUid:${uid}`);
	if (!code) {
		return false;
	}
	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj || !confirmObj.uid || parseInt(confirmObj.uid, 10) !== parseInt(uid, 10)) {
		return false;
	}
	if (email) {
		return !!(confirmObj && email === confirmObj.email && confirmObj.expires > Date.now());
	}
	return !!(confirmObj && confirmObj.expires > Date.now());
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
	const code = await db.get(`confirm:byUid:${uid}`);
	if (!code) {
		return null;
	}
	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj || !confirmObj.expires) {
		return null;
	}
	const expires = parseInt(confirmObj.expires, 10);
	if (expires <= Date.now()) {
		return null;
	}
	return expires - Date.now();
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
	if (ttl === null) {
		return true;
	}
	const max = meta.config.emailConfirmExpiry * 60 * 60 * 1000;
	const interval = meta.config.emailConfirmInterval * 60 * 1000;

	return ttl + interval < max;
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
	await db.setObject(`confirm:${confirm_code}`, {
		email: options.email.toLowerCase(),
		uid: uid,
	});
	await db.pexpire(`confirm:${confirm_code}`, emailConfirmExpiry * 60 * 60 * 1000);
=======
	await db.setObject(`confirm:${confirm_code}`, {
		email: options.email.toLowerCase(),
		uid: uid,
		expires: Date.now() + (emailConfirmExpiry * 60 * 60 * 1000),
	});
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
	if (confirmObj && confirmObj.uid && parseInt(confirmObj.uid, 10) === parseInt(uid, 10)) {
		return confirmObj.email;
	}
	return null;
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
	if (confirmObj && confirmObj.uid && parseInt(confirmObj.uid, 10) === parseInt(uid, 10)) {
		return confirmObj.email;
	}
	return null;
};
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

	// If another uid has the same email throw error
	const oldUid = await db.sortedSetScore('email:uid', currentEmail.toLowerCase());
	if (oldUid && oldUid !== parseInt(uid, 10)) {
		throw new Error('[[error:email-taken]]');
	}

	const confirmedEmails = await db.getSortedSetRangeByScore(`email:uid`, 0, -1, uid, uid);
	if (confirmedEmails.length) {
		// remove old email of user by uid
		await db.sortedSetsRemoveRangeByScore([`email:uid`], uid, uid);
		await db.sortedSetRemoveBulk(
			confirmedEmails.map(email => [`email:sorted`, `${email.toLowerCase()}:${uid}`])
		);
	}
	await Promise.all([
		db.sortedSetAddBulk([
			['email:uid', uid, currentEmail.toLowerCase()],
			['email:sorted', 0, `${currentEmail.toLowerCase()}:${uid}`],
			[`user:${uid}:emails`, Date.now(), `${currentEmail}:${Date.now()}`],
		]),
		user.setUserField(uid, 'email:confirmed', 1),
		groups.join('verified-users', uid),
		groups.leave('unverified-users', uid),
		user.email.expireValidation(uid),
		user.reset.cleanByUid(uid),
	]);
	await plugins.hooks.fire('action:user.email.confirmed', { uid: uid, email: currentEmail });
};
=======
UserEmail.confirmByUid = async function (uid) {
	if (!(parseInt(uid, 10) > 0)) {
		throw new Error('[[error:invalid-uid]]');
	}
	let currentEmail = await user.getUserField(uid, 'email');
	if (!currentEmail) {
		// fallback to email from pending confirmation
		currentEmail = await UserEmail.getEmailForValidation(uid);
		if (!currentEmail) {
			throw new Error('[[error:invalid-email]]');
		}
		// set the email to user profile
		await user.setUserField(uid, 'email', currentEmail);
	}

	// If another uid has the same email throw error
	const oldUid = await db.sortedSetScore('email:uid', currentEmail.toLowerCase());
	if (oldUid && oldUid !== parseInt(uid, 10)) {
		throw new Error('[[error:email-taken]]');
	}

	const confirmedEmails = await db.getSortedSetRangeByScore(`email:uid`, 0, -1, uid, uid);
	if (confirmedEmails.length) {
		// remove old email of user by uid
		await db.sortedSetsRemoveRangeByScore([`email:uid`], uid, uid);
		await db.sortedSetRemoveBulk(
			confirmedEmails.map(email => [`email:sorted`, `${email.toLowerCase()}:${uid}`])
		);
	}
	await Promise.all([
		db.sortedSetAddBulk([
			['email:uid', uid, currentEmail.toLowerCase()],
			['email:sorted', 0, `${currentEmail.toLowerCase()}:${uid}`],
			[`user:${uid}:emails`, Date.now(), `${currentEmail}:${Date.now()}`],
		]),
		user.setUserField(uid, 'email:confirmed', 1),
		groups.join('verified-users', uid),
		groups.leave('unverified-users', uid),
		user.email.expireValidation(uid),
		user.reset.cleanByUid(uid),
	]);
	await plugins.hooks.fire('action:user.email.confirmed', { uid: uid, email: currentEmail });
};
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

	if (!options.force && !await UserEmail.canSendValidation(uid, options.email)) {
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
	});
	await db.pexpire(`confirm:${confirm_code}`, emailConfirmExpiry * 60 * 60 * 1000);

	winston.verbose(`[user/email] Validation email for uid ${uid} sent to ${options.email}`);
	events.log({
		type: 'email-confirmation-sent',
		uid,
		confirm_code,
		...options,
	});

	if (plugins.hooks.hasListeners('action:user.verify')) {
		plugins.hooks.fire('action:user.verify', { uid: uid, data: data });
	} else {
		await emailer.send(data.template, uid, data);
	}
	return confirm_code;
};
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

	if (!options.force && !await UserEmail.canSendValidation(uid, options.email)) {
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
		expires: Date.now() + (emailConfirmExpiry * 60 * 60 * 1000),
	});

	winston.verbose(`[user/email] Validation email for uid ${uid} sent to ${options.email}`);
	events.log({
		type: 'email-confirmation-sent',
		uid,
		confirm_code,
		...options,
	});

	if (plugins.hooks.hasListeners('action:user.verify')) {
		plugins.hooks.fire('action:user.verify', { uid: uid, data: data });
	} else {
		await emailer.send(data.template, uid, data);
	}
	return confirm_code;
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.confirmByCode = async function (code, sessionId) {
	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj || !confirmObj.uid || !confirmObj.email) {
		throw new Error('[[error:invalid-data]]');
	}

	// If another uid has the same email, remove it
	const oldUid = await db.sortedSetScore('email:uid', confirmObj.email.toLowerCase());
	if (oldUid) {
		await UserEmail.remove(oldUid, sessionId);
	}

	const oldEmail = await user.getUserField(confirmObj.uid, 'email');
	if (oldEmail && confirmObj.email !== oldEmail) {
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
	if (!confirmObj || !confirmObj.uid || !confirmObj.email) {
		throw new Error('[[error:invalid-data]]');
	}

	// If another uid has the same email, remove it
	const oldUid = await db.sortedSetScore('email:uid', confirmObj.email.toLowerCase());
	if (oldUid) {
		await UserEmail.remove(oldUid, sessionId);
	}

	const oldEmail = await user.getUserField(confirmObj.uid, 'email');
	if (oldEmail && confirmObj.email !== oldEmail) {
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

### FILE: src/database/redis.js
<<<<<<< SEARCH
	mget: async function (keys) {
		if (!Array.isArray(keys) || !keys.length) {
			return [];
		}
		return await client.mget(keys);
	},
=======
	mget: async function (keys) {
		if (!Array.isArray(keys) || !keys.length) {
			return [];
		}
		const values = await client.mget(keys);
		return values.map(v => v === null ? null : v);
	},
>>>>>>> REPLACE

### FILE: src/database/mongo.js
<<<<<<< SEARCH
	mget: async function (keys) {
		if (!Array.isArray(keys) || !keys.length) {
			return [];
		}
		const objects = await client.collection('objects').find({ _key: { $in: keys } }).toArray();
		const map = {};
		objects.forEach(obj => {
			map[obj._key] = obj.value;
		});
		return keys.map(key => map[key] !== undefined ? map[key] : null);
	},
=======
	mget: async function (keys) {
		if (!Array.isArray(keys) || !keys.length) {
			return [];
		}
		const objects = await client.collection('objects').find({ _key: { $in: keys } }).toArray();
		const map = {};
		objects.forEach(obj => {
			map[obj._key] = obj.value;
		});
		return keys.map(key => map[key] !== undefined ? map[key] : null);
	},
>>>>>>> REPLACE

### FILE: src/database/postgres.js
<<<<<<< SEARCH
	mget: async function (keys) {
		if (!Array.isArray(keys) || !keys.length) {
			return [];
		}
		const res = await pool.query({
			name: 'mget',
			text: `
				SELECT o."_key", s."value"
				FROM "legacy_object_live" o
				LEFT JOIN "legacy_string" s ON o."_key" = s."_key"
				WHERE o."_key" = ANY($1)
			`,
			values: [keys],
		});
		const map = {};
		res.rows.forEach(row => {
			map[row._key] = row.value;
		});
		return keys.map(key => map[key] !== undefined ? map[key] : null);
	},
=======
	mget: async function (keys) {
		if (!Array.isArray(keys) || !keys.length) {
			return [];
		}
		const res = await pool.query({
			name: 'mget',
			text: `
				SELECT o."_key", s."value"
				FROM "legacy_object_live" o
				LEFT JOIN "legacy_string" s ON o."_key" = s."_key"
				WHERE o."_key" = ANY($1)
			`,
			values: [keys],
		});
		const map = {};
		res.rows.forEach(row => {
			map[row._key] = row.value;
		});
		return keys.map(key => map[key] !== undefined ? map[key] : null);
	},
>>>>>>> REPLACE