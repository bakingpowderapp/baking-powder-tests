use bp::project::{Cores, Host, Project};

#[test]
fn a_track_rack_requires_a_core_where_a_return_and_the_master_refuse_one() {
    let project = Project::new();
    assert_eq!(project.cores(Host::Track(0)), Cores::Required, "a track's rack does not require a Core");
    assert_eq!(project.cores(Host::Send(0)), Cores::Refused, "a send takes a Core");
    assert_eq!(project.cores(Host::Master), Cores::Refused, "the master takes a Core");
}
