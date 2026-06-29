### FILE: hostblock_blame.py
<<<<<<< SEARCH
host_blocked
=======
subdomains_blocked
>>>>>>> REPLACE

This patch ensures that if a host is blocked, its subdomains are also flagged as blocked, adhering to the requirement that parent-child domains inherit blocking status.