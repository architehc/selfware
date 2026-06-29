### FILE: src/user/email.js
<<<<<<< 
UserEmail.isValidationPending = async (uid, email) => {
	const code = await db.get(`confirm:byUid:${uid}`);

	if (email) {
		const confirmObj = await db.getObject(`confirm:${code}`);
		return !!(confirmObj && email === confirmObj.email);
	}

	return !!code;
};
>>>>>>> 
UserEmail.isValidationPending = async (uid, email) => {
	const code = await db.get(`confirm:byUid:${uid}`);

	if (email) {
		const confirmObj = await db.getObject(`confirm:${code}`);
		return !!(confirmObj && email === confirmObj.email && confirmObj.uid === uid);
	}

	return !!code;
};

### FILE: src/user/email.js
<<<<<<< 
UserEmail.getValidationExpiry = async (uid) => {
	const pending = await UserEmail.isValidationPending(uid);
	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
};
>>>>>>> 
UserEmail.getValidationExpiry = async (uid) => {
	const pending = await UserEmail.isValidationPending(uid);
	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
};

### FILE: src/user/email.js
<<<<<<< 
UserEmail.expireValidation = async (uid) => {
	const code = await db.get(`confirm:byUid:${uid}`);
	await db.deleteAll([
		`confirm:byUid:${uid}`,
		`confirm:${code}`,
	]);
};
>>>>>>> 
UserEmail.expireValidation = async (uid) => {
	const code = await db.get(`confirm:byUid:${uid}`);
	await db.deleteAll([
		`confirm:byUid:${uid}`,
		`confirm:${code}`,
	]);
};

### FILE: src/user/email.js
<<<<<<< 
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
>>>>>>> 
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

### FILE: src/api/users.js
<<<<<<< 
usersAPI.search = async function (caller, data) {
	if (!data) {
		throw new Error('[[error:invalid-data]]');
	}
	const [allowed, isPrivileged] = await Promise.all([
		privileges.global.can('search:users', caller.uid),
		user.isPrivileged(caller.uid),
	]);
	let filters = data.filters || [];
	filters = Array.isArray(filters) ? filters : [filters];
	if (!allowed ||
		((
			data.searchBy === 'ip' ||
			data.searchBy === 'email' ||
			filters.includes('banned') ||
			filters.includes('flagged')
		) && !isPrivileged)
	) {
		throw new Error('[[error:no-privileges]]');
	}
	return await user.search({
		query: data.query,
		searchBy: data.searchBy || 'username',
		page: data.page || 1,
		sortBy: data.sortBy || 'lastonline',
		filters: filters,
	});
};
>>>>>>> 
usersAPI.search = async function (caller, data) {
	if (!data) {
		throw new Error('[[error:invalid-data]]');
	}
	const [allowed, isPrivileged] = await Promise.all([
		privileges.global.can('search:users', caller.uid),
		user.isPrivileged(caller.uid),
	]);
	let filters = data.filters || [];
	filters = Array.isArray(filters) ? filters : [filters];
	if (!allowed ||
		((
			data.searchBy === 'ip' ||
			data.searchBy === 'email' ||
			filters.includes('banned') ||
			filters.includes('flagged')
		) && !isPrivileged)
	) {
		throw new Error('[[error:no-privileges]]');
	}
	return await user.search({
		query: data.query,
		searchBy: data.searchBy || 'username',
		page: data.page || 1,
		sortBy: data.sortBy || 'lastonline',
		filters: filters,
	});
};