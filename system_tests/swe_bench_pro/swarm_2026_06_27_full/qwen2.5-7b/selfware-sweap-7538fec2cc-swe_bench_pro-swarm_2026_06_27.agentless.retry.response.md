### FILE: src/user/email.js
<<<<<<< SEARCH
		});
	}
});
=======
		});
		const email = await user.email.getEmailForValidation(uid);
		if (email) {
			return email;
		}
		const confirmObj = await db.getObject(`confirm:byUid:${uid}`);
		if (confirmObj && confirmObj.email) {
			return confirmObj.email.toLowerCase();
		}
		return null;
	});
>>>>>>> REPLACE

### FILE: src/user/profile.js
<<<<<<< SEARCH
		}
	}
});
=======
		}
		const email = await user.email.getEmailForValidation(uid);
		if (email) {
			return email;
		}
		const confirmObj = await db.getObject(`confirm:byUid:${uid}`);
		if (confirmObj && confirmObj.email) {
			return confirmObj.email.toLowerCase();
		}
		return null;
	});
>>>>>>> REPLACE