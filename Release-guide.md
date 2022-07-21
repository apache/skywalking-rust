# Apache SkyWalking Rust SDK release guide
--------------------
If you're a committer, you can learn how to release SkyWalking SDK in The Apache Way and start the voting process by
reading this document.

## Requirements

- Rust(rustc)
- Cargo
- GPG
- shasum

## Add your GPG public key

1. Add your GPG public key into the [SkyWalking GPG KEYS](https://dist.apache.org/repos/dist/release/skywalking/KEYS)
   file. If you are a committer, use your Apache ID and password to log in this svn, and update the file. **Don't
   override the existing file.**(Notice, only PMC member could update this file)
2. Upload your GPG public key to the public GPG site, such as [MIT's site](http://pgp.mit.edu:11371/). This site should
   be in the Apache maven staging repository checklist.

## Test your settings

```shell
## Make sure local compiling passed
> cargo build
## Create local package. The *.crate should be found in {PROJECT_ROOT}/target/package/skywalking-x.y.z
> cargo package
```

## Sign the package

Tag the commit ID of this release as v`x.y.z`.

After set the version in `Cargo.toml` with the release number, package locally. Then run the following commands to sign
your package.

```shell
## The package should be signed by your Apache committer mail.
> gpg --armor --detach-sig skywalking-0.3.0.crate
## 
> export RELEASE_VERSION=x.y.z
> shasum -a 512 skywalking-{$RELEASE_VERSION}.crate > skywalking-{$RELEASE_VERSION}.crate.sha512
```

After these, the source tar with its signed asc and sha512 are ready.

## Upload to Apache SVN and tag a release

1. Use your Apache ID to log in to `https://dist.apache.org/repos/dist/dev/skywalking/rust`.
2. Create a folder and name it by the release version and round, such as: `x.y.z`
3. Upload tar ball, asc, sha512 files to the new folder.

## Call a vote in dev

Call a vote in `dev@skywalking.apache.org`

```
Mail title: [VOTE] Release Apache SkyWalking version Rust x.y.z

Mail content:
Hi All,
This is a call for vote to release Apache SkyWalking Rust version x.y.z.

Release Candidate:

* https://dist.apache.org/repos/dist/dev/skywalking/rust/x.y.z/
* sha512 checksums
- xxxxxxxx skywalking-x.y.z.crate

Release Tag :

* (Git Tag) vx.y.z

Release CommitID :

* https://github.com/apache/skywalking-rust/tree/{commit-id}
* Git submodule
* skywalking-data-collect-protocol
https://github.com/apache/skywalking-data-collect-protocol/tree/{commit-id}

Keys to verify the Release Candidate :

* https://dist.apache.org/repos/dist/release/skywalking/KEYS

Guide to build the release from source :

* https://github.com/apache/skywalking-rust#how-to-compile

Voting will start now (Date) and will remain open for at least 72
hours, Request all PMC members to give their vote.
[ ] +1 Release this package.
[ ] +0 No opinion.
[ ] -1 Do not release this package because....
```

## Vote Check

The voting process is as follows:

1. All PMC member votes are +1 binding, and all other votes are +1 but non-binding.
1. If you obtain at least 3 (+1 binding) votes with more +1 than -1 votes within 72 hours, the release will be approved.

## Publish the release

1. Move source codes tar and distribution packages to `https://dist.apache.org/repos/dist/release/skywalking/`.

```
> export SVN_EDITOR=vim
> svn mv https://dist.apache.org/repos/dist/dev/skywalking/rust/x.y.z https://dist.apache.org/repos/dist/release/skywalking/rust
....
enter your apache password
....
```

2. Cargo publish

```shell
# Publish binary on https://crates.io/crates/skywalking
# Make sure you are on `owner` list, reach private@skywalking.apache.org if you are a committer/PMC but not listed.
> cargo publish 
```

3. Add an release event, update download and doc releases on the SkyWalking website.
4. Add the new release on [ASF addrelease site](https://reporter.apache.org/addrelease.html?skywalking).
5. Remove the old releases on `https://dist.apache.org/repos/dist/release/skywalking/rust/{previous-version}`.

## Send a release announcement

Send ANNOUNCE email to `dev@skywalking.apache.org`, `announce@apache.org`. The sender should use the Apache email
account.

```
Mail title: [ANNOUNCE] Apache SkyWalking Rust x.y.z released

Mail content:
Hi all,

SkyWalking Rust Agent provides observability capability for Rust App and Library, including tracing, metrics, topology map for distributed system and alert.

SkyWalking: APM (application performance monitor) tool for distributed systems,
especially designed for microservices, cloud native and container-based (Docker, Kubernetes, Mesos) architectures.

This release contains a number of new features, bug fixes and improvements compared to
version a.b.c(last release). The notable changes since x.y.z include:

(Highlight key changes)
1. ...
2. ...
3. ...

Apache SkyWalking website:
http://skywalking.apache.org/

Downloads:
http://skywalking.apache.org/downloads/

Twitter:
https://twitter.com/ASFSkyWalking

SkyWalking Resources:
- GitHub: https://github.com/apache/skywalking
- Issue: https://github.com/apache/skywalking/issues
- Mailing list: dev@skywalkiing.apache.org


- Apache SkyWalking Team
```
