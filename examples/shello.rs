use dominator_static::StaticHtml;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dom = StaticHtml::from_str(
        r#"<div class="test">
            test<span>test2</span>
            <p style="font-weight:bold;">styled</p>
            <!-- a comment -->
            <escape>
            html!("div", {
                .class("test")
            })
            </escape>
        </div>
        "#,
        false,
    )?;
    println!("{}", dom.gen_dominator());
    Ok(())
}
