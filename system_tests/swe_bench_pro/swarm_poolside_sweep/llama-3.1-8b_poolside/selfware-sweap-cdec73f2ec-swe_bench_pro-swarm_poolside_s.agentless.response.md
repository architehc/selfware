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
	const confirmObj = await db.getObject(`confirm:${code}`);
	return confirmObj && (email? email === confirmObj.email : true);
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
	const confirmObj = await db.getObject(`confirm:${code}`);
	return confirmObj && confirmObj.expires? confirmObj.expires : null;
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
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.sendValidationEmail = async function (uid, options) {
	...
	const confirm_code = utils.generateUUID();
	const confirm_link = `${nconf.get('url')}/confirm/${confirm_code}`;

	const { emailConfirmInterval, emailConfirmExpiry } = meta.config;

	// If no email passed in (default), retrieve email from uid
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
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.confirmByCode = async function (code, sessionId) {
	const confirmObj = await db.getObject(`confirm:${code}`);
	if (!confirmObj ||!confirmObj.uid ||!confirmObj.email) {
		throw new Error('[[error:invalid-data]]');
	}

	// If another uid has the same email, remove it
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
	if (oldUid && oldUid!== parseInt(uid, 10)) {
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

### FILE: src/api/users.js
<<<<<<< SEARCH
usersAPI.update = async function (caller, data) {
	...
	if (data.hasOwnProperty('email') || data.hasOwnProperty('username')) {
		await isPrivilegedOrSelfAndPasswordMatch(caller, data);
	}
	...
>>>>>>> REPLACE

### FILE: src/api/users.js
<<<<<<< SEARCH
async function isPrivilegedOrSelfAndPasswordMatch(caller, data) {
	if (data.hasOwnProperty('email') || data.hasOwnProperty('username')) {
		if (caller.uid!== data.uid) {
			throw new Error('[[error:no-privileges]]');
		}
		const correct = await User.isPasswordCorrect(data.uid, data.currentPassword, data.ip);
		if (!correct) {
			throw new Error('[[user:change_password_error_wrong_current]]');
		}
	}
}
>>>>>>> REPLACE

### FILE: src/user/profile.js
<<<<<<< SEARCH
User.updateProfile = async function (uid, data, extraFields) {
	...
	if (field === 'email') {
		return await updateEmail(updateUid, data.email);
	} else if (field === 'username') {
		return await updateUsername(updateUid, data.username);
	}
	...
}
>>>>>>> REPLACE

### FILE: src/user/profile.js
<<<<<<< SEARCH
async function updateEmail(uid, newEmail) {
	let oldEmail = await User.getUserField(uid, 'email');
	oldEmail = oldEmail || '';
	if (oldEmail === newEmail) {
		return;
	}

	// 👉 Looking for email change logic? src/user/email.js (UserEmail.confirmByUid)
	if (newEmail) {
		await User.email.sendValidationEmail(uid, {
			email: newEmail,
			force: 1,
		}).catch(err => winston.error(`[user.create] Validation email failed to send\n[emailer.send] ${err.stack}`));
	}
}
>>>>>>> REPLACE