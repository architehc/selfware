
### FILE: src/database/redis/main.js
<<<<<<< SEARCH
module.exports = function (database) {
	const client = database.client;

	return {
		// ... existing methods
	};
};
=======
module.exports = function (database) {
	const client = database.client;

	return {
		mget: async function (keys) {
			if (!keys || !keys.length) {
				return [];
			}
			const values = await client.mget(keys);
			return keys.map((key, index) => values[index] || null);
		},
		// ... existing methods
	};
};
>>>>>>> REPLACE

### FILE: src/database/mongo/main.js
<<<<<<< SEARCH
module.exports = function (database) {
	const db = database.db;

	return {
		// ... existing methods
	};
};
=======
module.exports = function (database) {
	const db = database.db;

	return {
		mget: async function (keys) {
			if (!keys || !keys.length) {
				return [];
			}
			const objects = await db.collection('objects').find({ _key: { $in: keys } }).toArray();
			const keyToValue = {};
			objects.forEach(obj => {
				keyToValue[obj._key] = obj.value;
			});
			return keys.map(key => keyToValue[key] || null);
		},
		// ... existing methods
	};
};
>>>>>>> REPLACE

### FILE: src/database/postgres/main.js
<<<<<<< SEARCH
module.exports = function (database) {
	const client = database.client;

	return {
		// ... existing methods
	};
};
=======
module.exports = function (database) {
	const client = database.client;

	return {
		mget: async function (keys) {
			if (!keys || !keys.length) {
				return [];
			}
			const query = `
				SELECT o.key, s.value
				FROM legacy_object_live o
				LEFT JOIN legacy_string s ON o.key = s.key
				WHERE o.key = ANY($1)
			`;
			const result = await client.query(query, [keys]);
			const keyToValue = {};
			result.rows.forEach(row => {
				keyToValue[row.key] = row.value || null;
			});
			return keys.map(key => keyToValue[key] || null);
		},
		// ... existing methods
	};
};
>>>>>>> REPLACE

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
	const userEmail = await user.getUserField(uid, 'email');
	if (userEmail) {
		return userEmail;
	}

	const code = await db.get(`confirm:byUid:${uid}`);
	if (!code) {
		return null;
	}

	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj || confirmObj.uid !== parseInt(uid, 10)) {
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
	const code = await db.get(`confirm:byUid:${uid}`);
	if (!code) {
		return null;
	}

	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj) {
		return null;
	}

	const now = Date.now();
	if (confirmObj.expires && now >= confirmObj.expires) {
		return null;
	}

	return confirmObj.expires - now;
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
UserEmail.confirmByUid = async function (uid) {
	if (!(parseInt(uid, 10) > 0)) {
		throw new Error('[[error:invalid-uid]]');
	}

	const email = await UserEmail.getEmailForValidation(uid);
	if (!email) {
		throw new Error('[[error:invalid-email]]');
	}
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
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
=======
	// If another uid has the same email throw error
	const oldUid = await db.sortedSetScore('email:uid', email.toLowerCase());
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
			['email:uid', uid, email.toLowerCase()],
			['email:sorted', 0, `${email.toLowerCase()}:${uid}`],
			[`user:${uid}:emails`, Date.now(), `${email}:${Date.now()}`],
		]),
		user.setUserField(uid, 'email', email),
		user.setUserField(uid, 'email:confirmed', 1),
		groups.join('verified-users', uid),
		groups.leave('unverified-users', uid),
		user.email.expireValidation(uid),
		user.reset.cleanByUid(uid),
	]);
	await plugins.hooks.fire('action:user.email.confirmed', { uid: uid, email: email });
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

	const expires = Date.now() + (emailConfirmExpiry * 60 * 60 * 1000);
	await db.setObject(`confirm:${confirm_code}`, {
		email: options.email.toLowerCase(),
		uid: uid,
		expires: expires,
	});
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

	const expires = Date.now() + (emailConfirmExpiry * 60 * 60 * 1000);
	await db.setObject(`confirm:${confirm_code}`, {
		email: options.email.toLowerCase(),
		uid: uid,
		expires: expires,
	});
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

	// Check if confirmation has expired
	const now = Date.now();
	if (confirmObj.expires && now >= confirmObj.expires) {
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

### FILE: src/user/index.js
<<<<<<< SEARCH
User.validateEmail = async function (data) {
	const uid = data.uid;
	const email = data.email;

	if (!uid || !email) {
		throw new Error('[[error:invalid-data]]');
	}

	const userDoc = await user.getUserDoc(uid);
	if (!userDoc) {
		throw new Error('[[error:invalid-uid]]');
	}

	await user.email.confirmByUid(uid);
};
=======
User.validateEmail = async function (data) {
	const uid = data.uid;
	const email = data.email;

	if (!uid || !email) {
		throw new Error('[[error:invalid-data]]');
	}

	const userDoc = await user.getUserDoc(uid);
	if (!userDoc) {
		throw new Error('[[error:invalid-uid]]');
	}

	const confirmEmail = await user.email.getEmailForValidation(uid);
	if (confirmEmail) {
		await user.setUserField(uid, 'email', confirmEmail);
	}

	await user.email.confirmByUid(uid);
};
>>>>>>> REPLACE

### FILE: src/user/index.js
<<<<<<< SEARCH
User.create = async function (data) {
	const { username, email, password, ...extra } = data;
	const uid = await user.generateUid();

	await user.createUserDoc(uid, username, email, password, extra);
	await user.createUserIndex(uid, username, email);

	return uid;
};
=======
User.create = async function (data) {
	const { username, email, password, ...extra } = data;
	const uid = await user.generateUid();

	await user.createUserDoc(uid, username, email, password, extra);
	await user.createUserIndex(uid, username, email);

	// Expire any pending validation when creating user
	await user.email.expireValidation(uid);

	return uid;
};
>>>>>>> REPLACE

### FILE: src/api/users.js
<<<<<<< SEARCH
async function processDeletion({ uid, method, password, caller }) {
	const userData = await user.getUserFields(uid, ['email', 'username']);
	if (!userData || !userData.username) {
		throw new Error('[[error:invalid-data]]');
	}

	const [isAdmin, isGlobalMod] = await Promise.all([
		user.isAdminOrGlobalMod(caller.uid),
		user.isGlobalMod(caller.uid),
	]);

	const isSelf = parseInt(caller.uid, 10) === parseInt(uid, 10);
	const isEmailPrivate = await user.isEmailPrivate(uid);

	if (method === 'delete' && !isAdmin && !isGlobalMod && !isSelf) {
		throw new Error('[[error:no-privileges]]');
	}

	if (method === 'deleteContent' && !isAdmin && !isGlobalMod) {
		throw new Error('[[error:no-privileges]]');
	}

	if (method === 'deleteAccount' && !isAdmin && !isGlobalMod && !isSelf) {
		throw new Error('[[error:no-privileges]]');
	}

	if (!isSelf && !isAdmin && !isGlobalMod) {
		await user.checkMinReputation(caller.uid, uid, 'min:rep:delete');
	}

	if (method === 'deleteAccount') {
		await user.checkPassword(password, uid);
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
async function processDeletion({ uid, method, password, caller }) {
	const userData = await user.getUserFields(uid, ['email', 'username']);
	if (!userData || !userData.username) {
		throw new Error('[[error:invalid-data]]');
	}

	const [isAdmin, isGlobalMod] = await Promise.all([
		user.isAdminOrGlobalMod(caller.uid),
		user.isGlobalMod(caller.uid),
	]);

	const isSelf = parseInt(caller.uid, 10) === parseInt(uid, 10);
	const isEmailPrivate = await user.isEmailPrivate(uid);

	if (method === 'delete' && !isAdmin && !isGlobalMod && !isSelf) {
		throw new Error('[[error:no-privileges]]');
	}

	if (method === 'deleteContent' && !isAdmin && !isGlobalMod) {
		throw new Error('[[error:no-privileges]]');
	}

	if (method === 'deleteAccount' && !isAdmin && !isGlobalMod && !isSelf) {
		throw new Error('[[error:no-privileges]]');
	}

	if (!isSelf && !isAdmin && !isGlobalMod) {
		await user.checkMinReputation(caller.uid, uid, 'min:rep:delete');
	}

	if (method === 'deleteAccount') {
		await user.checkPassword(password, uid);
	}

	await events.log({
		type: `user-${method}`,
		uid: caller.uid,
		targetUid: uid,
		ip: caller.ip,
		username: userData.username,
		email: userData.email,
	});

	// Expire any pending email validation when deleting user
	await user.email.expireValidation(uid);
}
>>>>>>> REPLACE
