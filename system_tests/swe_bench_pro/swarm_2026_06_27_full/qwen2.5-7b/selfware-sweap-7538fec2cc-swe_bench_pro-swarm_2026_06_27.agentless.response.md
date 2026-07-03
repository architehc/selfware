### FILE: src/user/email.js
<<<<<<< SEARCH
const UserEmail = module.exports;

UserEmail.exists = async function (email) {
	const uid = await user.getUidByEmail(email.toLowerCase());
	return !!uid;
};

UserEmail.available = async function (email) {
	const exists = await db.isSortedSetMember('email:uid', email.toLowerCase());
	return !exists;
};

UserEmail.remove = async function (uid, sessionId) {
	const email = await user.getUserField(uid, 'email');
	if (!email) {
		return;
	}

	await Promise.all([
		user.setUserFields(uid, {
			email: '',
			'email:confirmed': 0,
		}),
		db.sortedSetRemove('email:uid', email.toLowerCase()),
		db.sortedSetRemove('email:sorted', `${email.toLowerCase()}:${uid}`),
		user.email.expireValidation(uid),
		sessionId ? user.auth.revokeAllSessions(uid, sessionId) : Promise.resolve(),
		events.log({ type: 'email-change', email, newEmail: '' }),
	]);
};

UserEmail.isValidationPending = async (uid, email) => {
	const code = await db.get(`confirm:byUid:${uid}`);

	if (email) {
		const confirmObj = await db.getObject(`confirm:${code}`);
		return !!(confirmObj && email === confirmObj.email);
	}

	return !!code;
};

UserEmail.getValidationExpiry = async (uid) => {
	const pending = await UserEmail.isValidationPending(uid);
	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
};

UserEmail.expireValidation = async (uid) => {
	const code = await db.get(`confirm:byUid:${uid}`);
	await db.deleteAll([
		`confirm:byUid:${uid}`,
		`confirm:${code}`,
	]);
};

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

// confirm email by code sent by confirmation email
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

// confirm uid's email via ACP
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
const UserEmail = module.exports;

UserEmail.exists = async function (email) {
	const uid = await user.getUidByEmail(email.toLowerCase());
	return !!uid;
};

UserEmail.available = async function (email) {
	const exists = await db.isSortedSetMember('email:uid', email.toLowerCase());
	return !exists;
};

UserEmail.remove = async function (uid, sessionId) {
	const email = await user.getUserField(uid, 'email');
	if (!email) {
		return;
	}

	await Promise.all([
		user.setUserFields(uid, {
			email: '',
			'email:confirmed': 0,
		}),
		db.sortedSetRemove('email:uid', email.toLowerCase()),
		db.sortedSetRemove('email:sorted', `${email.toLowerCase()}:${uid}`),
		user.email.expireValidation(uid),
		sessionId ? user.auth.revokeAllSessions(uid, sessionId) : Promise.resolve(),
		events.log({ type: 'email-change', email, newEmail: '' }),
	]);
};

UserEmail.isValidationPending = async (uid, email) => {
	const code = await db.get(`confirm:byUid:${uid}`);

	if (email) {
		const confirmObj = await db.getObject(`confirm:${code}`);
		return !!(confirmObj && email === confirmObj.email);
	}

	return !!code;
};

UserEmail.getValidationExpiry = async (uid) => {
	const pending = await UserEmail.isValidationPending(uid);
	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
};

UserEmail.expireValidation = async (uid) => {
	const code = await db.get(`confirm:byUid:${uid}`);
	await db.deleteAll([
		`confirm:byUid:${uid}`,
		`confirm:${code}`,
	]);
};

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

// confirm email by code sent by confirmation email
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

// confirm uid's email via ACP
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
>>>>>>> REPLACE

### FILE: test/user/emails.js
<<<<<<< SEARCH
describe('email confirmation (library methods)', () => {
	let uid;
	async function dummyEmailerHook(data) {
		// pretend to handle sending emails
	}

	before(() => {
		// Attach an emailer hook so related requests do not error
		plugins.hooks.register('emailer-test', {
			hook: 'filter:email.send',
			method: dummyEmailerHook,
		});
	});

	beforeEach(async () => {
		uid = await user.create({
			username: utils.generateUUID().slice(0, 10),
			password: utils.generateUUID(),
		});
	});

	after(async () => {
		plugins.hooks.unregister('emailer-test', 'filter:email.send');
	});

	describe('isValidationPending', () => {
		it('should return false if user did not request email validation', async () => {
			const pending = await user.email.isValidationPending(uid);

			assert.strictEqual(pending, false);
		});

		it('should return false if user did not request email validation (w/ email checking)', async () => {
			const email = 'test@example.org';
			const pending = await user.email.isValidationPending(uid, email);

			assert.strictEqual(pending, false);
		});

		it('should return true if user requested email validation', async () => {
			const email = 'test@example.org';
			await user.email.sendValidationEmail(uid, {
				email,
			});
			const pending = await user.email.isValidationPending(uid);

			assert.strictEqual(pending, true);
		});

		it('should return true if user requested email validation (w/ email checking)', async () => {
			const email = 'test@example.org';
			await user.email.sendValidationEmail(uid, {
				email,
			});
			const pending = await user.email.isValidationPending(uid, email);

			assert.strictEqual(pending, true);
		});
	});

	describe('getValidationExpiry', () => {
		it('should return null if there is no validation available', async () => {
			const expiry = await user.email.getValidationExpiry(uid);

			assert.strictEqual(expiry, null);
		});

		it('should return a number smaller than configured expiry if validation available', async () => {
			const email = 'test@example.org';
			await user.email.sendValidationEmail(uid, {
				email,
			});
			const expiry = await user.email.getValidationExpiry(uid);

			assert(isFinite(expiry));
			assert(expiry > 0);
			assert(expiry <= meta.config.emailConfirmExpiry * 24 * 60 * 60 * 1000);
		});
	});

	describe('expireValidation', () => {
		it('should invalidate any confirmation in-progress', async () => {
			const email = 'test@example.org';
			await user.email.sendValidationEmail(uid, {
				email,
			});
			await user.email.expireValidation(uid);

			assert.strictEqual(await user.email.isValidationPending(uid), false);
			assert.strictEqual(await user.email.isValidationPending(uid, email), false);
			assert.strictEqual(await user.email.canSendValidation(uid, email), true);
		});
	});

	describe('canSendValidation', () => {
		it('should return true if no validation is pending', async () => {
			const ok = await user.email.canSendValidation(uid, 'test@example.com');

			assert(ok);
		});

		it('should return false if it has been too soon to re-send confirmation', async () => {
			const email = 'test@example.org';
			await user.email.sendValidationEmail(uid, {
				email,
			});
			const ok = await user.email.canSendValidation(uid, email);

			assert.strictEqual(ok, false);
		});

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
	});
});
=======
describe('email confirmation (library methods)', () => {
	let uid;
	async function dummyEmailerHook(data) {
		// pretend to handle sending emails
	}

	before(() => {
		// Attach an emailer hook so related requests do not error
		plugins.hooks.register('emailer-test', {
			hook: 'filter:email.send',
			method: dummyEmailerHook,
		});
	});

	beforeEach(async () => {
		uid = await user.create({
			username: utils.generateUUID().slice(0, 10),
			password: utils.generateUUID(),
		});
	});

	after(async () => {
		plugins.hooks.unregister('emailer-test', 'filter:email.send');
	});

	describe('isValidationPending', () => {
		it('should return false if user did not request email validation', async () => {
			const pending = await user.email.isValidationPending(uid);

			assert.strictEqual(pending, false);
		});

		it('should return false if user did not request email validation (w/ email checking)', async () => {
			const email = 'test@example.org';
			const pending = await user.email.isValidationPending(uid, email);

			assert.strictEqual(pending, false);
		});

		it('should return true if user requested email validation', async () => {
			const email = 'test@example.org';
			await user.email.sendValidationEmail(uid, {
				email,
			});
			const pending = await user.email.isValidationPending(uid);

			assert.strictEqual(pending, true);
		});

		it('should return true if user requested email validation (w/ email checking)', async () => {
			const email = 'test@example.org';
			await user.email.sendValidationEmail(uid, {
				email,
			});
			const pending = await user.email.isValidationPending(uid, email);

			assert.strictEqual(pending, true);
		});
	});

	describe('getValidationExpiry', () => {
		it('should return null if there is no validation available', async () => {
			const expiry = await user.email.getValidationExpiry(uid);

			assert.strictEqual(expiry, null);
		});

		it('should return a number smaller than configured expiry if validation available', async () => {
			const email = 'test@example.org';
			await user.email.sendValidationEmail(uid, {
				email,
			});
			const expiry = await user.email.getValidationExpiry(uid);

			assert(isFinite(expiry));
			assert(expiry > 0);
			assert(expiry <= meta.config.emailConfirmExpiry * 24 * 60 * 60 * 1000);
		});
	});

	describe('expireValidation', () => {
		it('should invalidate any confirmation in-progress', async () => {
			const email = 'test@example.org';
			await user.email.sendValidationEmail(uid, {
				email,
			});
			await user.email.expireValidation(uid);

			assert.strictEqual(await user.email.isValidationPending(uid), false);
			assert.strictEqual(await user.email.isValidationPending(uid, email), false);
			assert.strictEqual(await user.email.canSendValidation(uid, email), true);
		});
	});

	describe('canSendValidation', () => {
		it('should return true if no validation is pending', async () => {
			const ok = await user.email.canSendValidation(uid, 'test@example.com');

			assert(ok);
		});

		it('should return false if it has been too soon to re-send confirmation', async () => {
			const email = 'test@example.org';
			await user.email.sendValidationEmail(uid, {
				email,
			});
			const ok = await user.email.canSendValidation(uid, email);

			assert.strictEqual(ok, false);
		});

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
	});
});
>>>>>>> REPLACE