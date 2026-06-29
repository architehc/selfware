### FILE: src/user/email.js
<<<<<<< SEARCH
		const confirmObj = await db.getObject(`confirm:${code}`);
=======
		const confirmObj = await db.getObject(`confirm:${code}`);
		if (!confirmObj || !confirmObj.uid || !confirmObj.email) {
			throw new Error('[[error:invalid-data]]');
		}
>>>>>>> REPLACE

### FILE: src/api/users.js
<<<<<<< SEARCH
		const { displayname } = userData;
=======
		const { displayname } = userData;
		if (!displayname) {
			displayname = userData.username;
		}
>>>>>>> REPLACE

### FILE: src/user/profile.js
<<<<<<< SEARCH
	async function isEmailValid(data) {
		if (!data.email) {
			return;
		}

		data.email = data.email.trim();
		if (!utils.isEmailValid(data.email)) {
			throw new Error('[[error:invalid-email]]');
		}
	}
=======
	async function isEmailValid(data) {
		if (!data.email) {
			return;
		}

		data.email = data.email.trim();
		if (!utils.isEmailValid(data.email)) {
			throw new Error('[[error:invalid-email]]');
		}

		if (data.uid && data.email !== await User.getUserField(data.uid, 'email')) {
			throw new Error('[[error:email-change-not-allowed]]');
		}
	}
>>>>>>> REPLACE