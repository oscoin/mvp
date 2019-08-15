module Styles = {
  open Css;

  let app = style([color(Particle.Color.black()), ...Particle.Font.text]);
};

[@react.component]
let make = () => {
  open Page;
  open Router;

  let page =
    switch (currentPage()) {
    | Root => <Generic title="Home of Oscoin" />
    | Projects => <Projects />
    | Project(id) => <Project id subPage=Project.Overview />
    | ProjectCode(id) => <Project id subPage=Project.Code />
    | ProjectFunds(id) => <Project id subPage=Project.Funds />
    | NotFound(_path) => <Generic title="Not Found" />
    };

  <div className=Styles.app> <Topbar /> page <Footer /> </div>;
};
