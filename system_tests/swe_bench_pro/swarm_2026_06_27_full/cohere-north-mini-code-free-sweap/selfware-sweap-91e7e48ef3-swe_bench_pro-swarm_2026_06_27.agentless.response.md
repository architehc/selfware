### FILE: src/user/index.js
<<<<<<< SEARCH
=======
"use strict";

const db = require('../database');
const user = require('./index');
const utils = require('../utils');
const plugins = require('../plugins');
const meta = require('../meta');
const groups = require('../groups');
const events = require('../events');
const privileges = require('../privileges');
const sockets = require('../socket.io');
const translator = require('../translator');
const validator = require('validator');
const slugify = require('../slugify');
const file = require('../file');
const messaging = require('../messaging');
const topics = require('../topics');
const categories = require('../categories');
const posts = require('../posts');
const notifications = require('../notifications');
const flags = require('../flags');
const emailer = require('../emailer');
const nconf = require('nconf');
const winston = require('winston');

const User = module.exports;

User.getUserData = async function (uid) {
    const userData = await db.getObject(`user:${uid}`);
    if (!userData) {
        throw new Error('[[error:invalid-uid]]');
    }
    return userData;
};

User.getUserFields = async function (uid, fields) {
    const userData = await db.getObject(`user:${uid}`);
    if (!userData) {
        return null;
    }
    const result = {};
    fields.forEach(field => {
        if (userData.hasOwnProperty(field)) {
            result[field] = userData[field];
        }
    });
    return result;
};

User.setUserFields = async function (uid, fields) {
    await db.setObjectFields(`user:${uid}`, fields);
};

User.getUserField = async function (uid, field) {
    return await db.getObjectField(`user:${uid}`, field);
};

User.exists = async function (username) {
    return await db.isSortedSetMember('username:uid', username);
};

User.existsBySlug = async function (userslug) {
    return await db.isSortedSetMember('userslug:uid', userslug);
};

User.getUidByUsername = async function (username) {
    return await db.getSortedSetScore('username:uid', username);
};

User.getUidByUserslug = async function (userslug) {
    return await db.getSortedSetScore('userslug:uid', userslug);
};

User.getUidByEmail = async function (email) {
    return await db.getSortedSetScore('email:uid', email.toLowerCase());
};

User.getUserDataByUsername = async function (username) {
    const uid = await User.getUidByUsername(username);
    if (!uid) {
        return null;
    }
    return await User.getUserData(uid);
};

User.getUserDataByUserslug = async function (userslug) {
    const uid = await User.getUidByUserslug(userslug);
    if (!uid) {
        return null;
    }
    return await User.getUserData(uid);
};

User.getUserDataByEmail = async function (email) {
    const uid = await User.getUidByEmail(email);
    if (!uid) {
        return null;
    }
    return await User.getUserData(uid);
};

User.getUserFieldsByUsername = async function (username, fields) {
    const uid = await User.getUidByUsername(username);
    if (!uid) {
        return null;
    }
    return await User.getUserFields(uid, fields);
};

User.getUserFieldsByUserslug = async function (userslug, fields) {### FILE: src/user/index.js
<<<<<<< SEARCH
    return await User.getUserFieldsByUserslug(userslug, fields);
};

User.loadUserInfo = async function (callerUid, uids) {
    const userData = await db.getObjects(`user:${uids.map(uid => uid).join(',')}`);
    const userInfo = {};
    for (const uid of uids) {
        const data = userData[uid] || {};
        userInfo[uid] = {
            uid: parseInt(uid, 10),
            username: data.username || '',
            userslug: data.userslug || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data.email || '',
            email: data